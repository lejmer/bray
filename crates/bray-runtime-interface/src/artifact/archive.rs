use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};
use std::path::Path;

use bray_base::Sha256Reader;

use super::RuntimeArtifactDigest;

pub(super) enum ArchiveValidationError {
    Unreadable(io::ErrorKind),
    Invalid,
}

pub(super) fn authenticate_archive(
    path: &Path,
) -> Result<RuntimeArtifactDigest, ArchiveValidationError> {
    let archive = std::fs::File::open(path)
        .map_err(|error| ArchiveValidationError::Unreadable(error.kind()))?;

    let metadata = archive
        .metadata()
        .map_err(|error| ArchiveValidationError::Unreadable(error.kind()))?;

    if !metadata.is_file() {
        return Err(ArchiveValidationError::Invalid);
    }

    let mut archive = Sha256Reader::new(archive);
    let mut magic = [0; 8];

    read_archive_exact(&mut archive, &mut magic)?;

    match &magic {
        b"!<arch>\n" => validate_unix_archive(&mut archive)?,
        b"<bigaf>\n" => validate_big_archive(&mut archive)?,
        _ => return Err(ArchiveValidationError::Invalid),
    }

    Ok(RuntimeArtifactDigest::new(archive.finalize()))
}

fn validate_unix_archive(archive: &mut impl Read) -> Result<(), ArchiveValidationError> {
    loop {
        let mut header = [0; 60];

        match archive.read(&mut header[..1]) {
            Ok(0) => return Ok(()),
            Ok(1) => read_archive_exact(archive, &mut header[1..])?,
            Ok(_) => unreachable!("a one-byte read cannot return more than one byte"),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(ArchiveValidationError::Unreadable(error.kind())),
        }

        let member_size = decimal_field(&header[48..58])?;

        if header[58..] != *b"`\n" {
            return Err(ArchiveValidationError::Invalid);
        }

        consume_archive_bytes(archive, member_size)?;

        if member_size % 2 == 1 {
            read_archive_padding(archive, b'\n')?;
        }
    }
}

fn validate_big_archive(archive: &mut impl Read) -> Result<(), ArchiveValidationError> {
    const FIXED_HEADER_REMAINDER: usize = 120;
    const FIXED_HEADER_LENGTH: u64 = 128;

    let mut fixed = [0; FIXED_HEADER_REMAINDER];

    read_archive_exact(archive, &mut fixed)?;

    let member_table = decimal_field(&fixed[0..20])?;
    let global_symbols = decimal_field(&fixed[20..40])?;
    let global_symbols_64 = decimal_field(&fixed[40..60])?;
    let first_member = decimal_field(&fixed[60..80])?;
    let last_member = decimal_field(&fixed[80..100])?;
    let first_free_member = decimal_field(&fixed[100..120])?;

    if (first_member == 0) != (last_member == 0) {
        return Err(ArchiveValidationError::Invalid);
    }

    let mut pending = [
        member_table,
        global_symbols,
        global_symbols_64,
        first_member,
        last_member,
        first_free_member,
    ]
    .into_iter()
    .filter(|offset| *offset != 0)
    .collect::<BTreeSet<_>>();

    let mut members = BTreeMap::new();
    let mut position = FIXED_HEADER_LENGTH;

    while let Some(offset) = pending.pop_first() {
        if members.contains_key(&offset) {
            continue;
        }

        if offset < position || offset % 2 != 0 {
            return Err(ArchiveValidationError::Invalid);
        }

        consume_archive_padding(archive, offset - position)?;

        let member = read_big_archive_member(archive)?;

        position = offset
            .checked_add(member.encoded_length)
            .ok_or(ArchiveValidationError::Invalid)?;

        for referenced in [member.next, member.previous] {
            if referenced != 0 {
                pending.insert(referenced);
            }
        }

        members.insert(offset, member);
    }

    for (offset, member) in &members {
        if member.next != 0
            && members
                .get(&member.next)
                .is_none_or(|next| next.previous != *offset)
        {
            return Err(ArchiveValidationError::Invalid);
        }

        if member.previous != 0
            && members
                .get(&member.previous)
                .is_none_or(|previous| previous.next != *offset)
        {
            return Err(ArchiveValidationError::Invalid);
        }
    }

    validate_big_archive_fixed_links(
        &members,
        member_table,
        global_symbols,
        global_symbols_64,
        first_member,
        last_member,
        first_free_member,
    )?;

    ensure_archive_end(archive)
}

struct BigArchiveMember {
    next: u64,
    previous: u64,
    encoded_length: u64,
}

#[expect(
    clippy::too_many_arguments,
    reason = "each argument is one named offset from the fixed AIX archive header"
)]
fn validate_big_archive_fixed_links(
    members: &BTreeMap<u64, BigArchiveMember>,
    member_table: u64,
    global_symbols: u64,
    global_symbols_64: u64,
    first_member: u64,
    last_member: u64,
    first_free_member: u64,
) -> Result<(), ArchiveValidationError> {
    if first_member == 0 {
        if member_table != 0
            || global_symbols != 0
            || global_symbols_64 != 0
            || last_member != 0
        {
            return Err(ArchiveValidationError::Invalid);
        }
    } else {
        if member_table == 0 {
            return Err(ArchiveValidationError::Invalid);
        }

        let first = members
            .get(&first_member)
            .ok_or(ArchiveValidationError::Invalid)?;

        let last = members
            .get(&last_member)
            .ok_or(ArchiveValidationError::Invalid)?;

        let table = members
            .get(&member_table)
            .ok_or(ArchiveValidationError::Invalid)?;

        let after_table = if global_symbols != 0 {
            global_symbols
        } else {
            global_symbols_64
        };

        if first.previous != 0
            || last.next != member_table
            || table.previous != last_member
            || table.next != after_table
        {
            return Err(ArchiveValidationError::Invalid);
        }

        if global_symbols != 0 {
            let symbols = members
                .get(&global_symbols)
                .ok_or(ArchiveValidationError::Invalid)?;

            if symbols.previous != member_table || symbols.next != global_symbols_64 {
                return Err(ArchiveValidationError::Invalid);
            }
        }

        if global_symbols_64 != 0 {
            let symbols = members
                .get(&global_symbols_64)
                .ok_or(ArchiveValidationError::Invalid)?;

            let previous = if global_symbols != 0 {
                global_symbols
            } else {
                member_table
            };

            if symbols.previous != previous || symbols.next != 0 {
                return Err(ArchiveValidationError::Invalid);
            }
        } else if global_symbols == 0 && table.next != 0 {
            return Err(ArchiveValidationError::Invalid);
        }
    }

    let main_chain = big_archive_chain(members, first_member)?;
    let free_chain = big_archive_chain(members, first_free_member)?;

    if !main_chain.is_disjoint(&free_chain)
        || main_chain.len() + free_chain.len() != members.len()
    {
        return Err(ArchiveValidationError::Invalid);
    }

    Ok(())
}

fn big_archive_chain(
    members: &BTreeMap<u64, BigArchiveMember>,
    first: u64,
) -> Result<BTreeSet<u64>, ArchiveValidationError> {
    let mut chain = BTreeSet::new();
    let mut current = first;

    while current != 0 {
        if !chain.insert(current) {
            return Err(ArchiveValidationError::Invalid);
        }

        let member = members
            .get(&current)
            .ok_or(ArchiveValidationError::Invalid)?;

        current = member.next;
    }

    Ok(chain)
}

fn read_big_archive_member(
    archive: &mut impl Read,
) -> Result<BigArchiveMember, ArchiveValidationError> {
    const FIXED_MEMBER_HEADER_LENGTH: u64 = 112;

    let mut header = [0; 112];

    read_archive_exact(archive, &mut header)?;

    let size = decimal_field(&header[0..20])?;
    let next = decimal_field(&header[20..40])?;
    let previous = decimal_field(&header[40..60])?;
    let name_length = decimal_field(&header[108..112])?;
    let padded_name_length = aligned_even(name_length)?;

    consume_archive_bytes(archive, name_length)?;
    consume_archive_padding(archive, padded_name_length - name_length)?;

    let mut terminator = [0; 2];

    read_archive_exact(archive, &mut terminator)?;

    if terminator != *b"`\n" {
        return Err(ArchiveValidationError::Invalid);
    }

    consume_archive_bytes(archive, size)?;

    let padded_size = aligned_even(size)?;

    consume_archive_padding(archive, padded_size - size)?;

    let encoded_length = FIXED_MEMBER_HEADER_LENGTH
        .checked_add(padded_name_length)
        .and_then(|length| length.checked_add(2))
        .and_then(|length| length.checked_add(padded_size))
        .ok_or(ArchiveValidationError::Invalid)?;

    Ok(BigArchiveMember {
        next,
        previous,
        encoded_length,
    })
}

fn aligned_even(value: u64) -> Result<u64, ArchiveValidationError> {
    value
        .checked_add(value % 2)
        .ok_or(ArchiveValidationError::Invalid)
}

fn decimal_field(bytes: &[u8]) -> Result<u64, ArchiveValidationError> {
    let value = std::str::from_utf8(bytes)
        .map_err(|_| ArchiveValidationError::Invalid)?
        .trim_end_matches(' ');

    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ArchiveValidationError::Invalid);
    }

    value.parse().map_err(|_| ArchiveValidationError::Invalid)
}

fn consume_archive_bytes(
    archive: &mut impl Read,
    mut remaining: u64,
) -> Result<(), ArchiveValidationError> {
    consume_archive_chunks(archive, &mut remaining, |_| true)
}

fn consume_archive_padding(
    archive: &mut impl Read,
    mut remaining: u64,
) -> Result<(), ArchiveValidationError> {
    consume_archive_chunks(archive, &mut remaining, |bytes| {
        bytes.iter().all(|byte| *byte == 0)
    })
}

fn consume_archive_chunks(
    archive: &mut impl Read,
    remaining: &mut u64,
    valid: impl Fn(&[u8]) -> bool,
) -> Result<(), ArchiveValidationError> {
    const BUFFER_LENGTH: usize = 64 * 1024;

    let mut buffer = [0; BUFFER_LENGTH];

    let buffer_length = u64::try_from(BUFFER_LENGTH)
        .unwrap_or_else(|_| unreachable!("archive buffer length must fit u64"));

    while *remaining != 0 {
        let requested = usize::try_from((*remaining).min(buffer_length))
            .unwrap_or_else(|_| unreachable!("bounded archive read length must fit usize"));

        let length = match archive.read(&mut buffer[..requested]) {
            Ok(length) => length,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(ArchiveValidationError::Unreadable(error.kind())),
        };

        if length == 0 || !valid(&buffer[..length]) {
            return Err(ArchiveValidationError::Invalid);
        }

        *remaining -= u64::try_from(length)
            .unwrap_or_else(|_| unreachable!("archive read length must fit u64"));
    }

    Ok(())
}

fn read_archive_padding(
    archive: &mut impl Read,
    expected: u8,
) -> Result<(), ArchiveValidationError> {
    let mut padding = [0];

    read_archive_exact(archive, &mut padding)?;

    if padding == [expected] {
        Ok(())
    } else {
        Err(ArchiveValidationError::Invalid)
    }
}

fn read_archive_exact(
    archive: &mut impl Read,
    bytes: &mut [u8],
) -> Result<(), ArchiveValidationError> {
    match archive.read_exact(bytes) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
            Err(ArchiveValidationError::Invalid)
        }
        Err(error) => Err(ArchiveValidationError::Unreadable(error.kind())),
    }
}

fn ensure_archive_end(archive: &mut impl Read) -> Result<(), ArchiveValidationError> {
    let mut byte = [0];

    loop {
        match archive.read(&mut byte) {
            Ok(0) => return Ok(()),
            Ok(_) => return Err(ArchiveValidationError::Invalid),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(ArchiveValidationError::Unreadable(error.kind())),
        }
    }
}
