use bray_runtime_interface::{
    NativePlatformDateTime, NativePlatformStatus, NativePlatformTemporalObservation,
    NativePlatformTemporalResolution, NativePlatformTemporalValue, NativePlatformText,
};

use super::region::{MemoryRegion, disjoint};

#[expect(
    unsafe_code,
    reason = "the pinned C++ temporal provider is called only through validated fixed-layout values"
)]
unsafe extern "C" {
    fn bray_temporal_date_validate(year: i32, month: u32, day: u32) -> u32;
    fn bray_temporal_date_add(
        value: NativePlatformDateTime,
        years: i32,
        months: i32,
        days: i32,
        adjustment: u32,
        result: *mut NativePlatformDateTime,
    ) -> u32;
    fn bray_temporal_zone_load(name: *const u8, length: u64, handle: *mut u64) -> u32;
    fn bray_temporal_zone_local(handle: *mut u64) -> u32;
    fn bray_temporal_zone_retain(handle: u64) -> u32;
    fn bray_temporal_zone_close(handle: u64) -> u32;
    fn bray_temporal_zone_name(
        handle: u64,
        destination: *mut u8,
        capacity: u64,
        written_or_required: *mut u64,
    ) -> u32;
    fn bray_temporal_observe(
        zone: u64,
        fixed_offset_seconds: i32,
        seconds: i64,
        nanoseconds: u32,
        observation: *mut NativePlatformTemporalObservation,
        abbreviation: *mut u8,
        capacity: u64,
        written_or_required: *mut u64,
    ) -> u32;
    fn bray_temporal_resolve(
        zone: u64,
        fixed_offset_seconds: i32,
        local: NativePlatformDateTime,
        resolution: *mut NativePlatformTemporalResolution,
    ) -> u32;
    fn bray_temporal_parse(
        kind: u32,
        text: *const u8,
        length: u64,
        value: *mut NativePlatformTemporalValue,
        invalid_offset: *mut u64,
    ) -> u32;
    fn bray_temporal_format(
        kind: u32,
        value: NativePlatformTemporalValue,
        destination: *mut u8,
        capacity: u64,
        written_or_required: *mut u64,
    ) -> u32;
}

native_platform_export! {
    pub extern "C" fn bray_platform_time_date_validate(
        year: i32,
        month: u32,
        day: u32,
        outcome: *mut u32,
    ) -> NativePlatformStatus {
        if MemoryRegion::write(outcome).is_none() {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let result = unsafe { bray_temporal_date_validate(year, month, day) };

        unsafe { outcome.write(result) };

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_time_date_add(
        value: NativePlatformDateTime,
        years: i32,
        months: i32,
        days: i32,
        adjustment: u32,
        result: *mut NativePlatformDateTime,
        outcome: *mut u32,
    ) -> NativePlatformStatus {
        let Some(result_region) = MemoryRegion::write(result) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(outcome_region) = MemoryRegion::write(outcome) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if result_region.overlaps(outcome_region) {
            return NativePlatformStatus::INVALID_INPUT;
        }

        unsafe {
            result.write(NativePlatformDateTime::default());
            outcome.write(bray_temporal_date_add(
                value,
                years,
                months,
                days,
                adjustment,
                result,
            ));
        }

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_time_zone_load(
        name: NativePlatformText,
        handle: *mut u64,
        outcome: *mut u32,
    ) -> NativePlatformStatus {
        let Ok(length) = usize::try_from(name.length()) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(name_region) = MemoryRegion::read(name.address(), length) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(handle_region) = MemoryRegion::write(handle) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(outcome_region) = MemoryRegion::write(outcome) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if !disjoint(&[name_region, handle_region, outcome_region]) {
            return NativePlatformStatus::INVALID_INPUT;
        }

        unsafe {
            handle.write(0);
            outcome.write(bray_temporal_zone_load(
                name.address(),
                name.length(),
                handle,
            ));
        }

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_time_zone_local(
        handle: *mut u64,
        outcome: *mut u32,
    ) -> NativePlatformStatus {
        let Some(handle_region) = MemoryRegion::write(handle) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(outcome_region) = MemoryRegion::write(outcome) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if handle_region.overlaps(outcome_region) {
            return NativePlatformStatus::INVALID_INPUT;
        }

        unsafe {
            handle.write(0);
            outcome.write(bray_temporal_zone_local(handle));
        }

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_time_zone_retain(handle: u64) -> NativePlatformStatus {
        provider_status(unsafe { bray_temporal_zone_retain(handle) })
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_time_zone_close(handle: u64) -> NativePlatformStatus {
        provider_status(unsafe { bray_temporal_zone_close(handle) })
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_time_zone_name(
        handle: u64,
        destination: *mut u8,
        capacity: u64,
        written_or_required: *mut u64,
        outcome: *mut u32,
    ) -> NativePlatformStatus {
        let Some(regions) = variable_output_regions(destination, capacity, written_or_required, outcome)
        else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if !disjoint(&regions) {
            return NativePlatformStatus::INVALID_INPUT;
        }

        unsafe {
            written_or_required.write(0);
            outcome.write(bray_temporal_zone_name(
                handle,
                destination,
                capacity,
                written_or_required,
            ));
        }

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_time_observe(
        zone: u64,
        fixed_offset_seconds: i32,
        seconds: i64,
        nanoseconds: u32,
        observation: *mut NativePlatformTemporalObservation,
        abbreviation: *mut u8,
        capacity: u64,
        written_or_required: *mut u64,
        outcome: *mut u32,
    ) -> NativePlatformStatus {
        let Some(observation_region) = MemoryRegion::write(observation) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(variable_regions) =
            variable_output_regions(abbreviation, capacity, written_or_required, outcome)
        else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if variable_regions
            .iter()
            .any(|region| observation_region.overlaps(*region))
            || !disjoint(&variable_regions)
        {
            return NativePlatformStatus::INVALID_INPUT;
        }

        unsafe {
            observation.write(NativePlatformTemporalObservation::default());
            written_or_required.write(0);
            outcome.write(bray_temporal_observe(
                zone,
                fixed_offset_seconds,
                seconds,
                nanoseconds,
                observation,
                abbreviation,
                capacity,
                written_or_required,
            ));
        }

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_time_resolve(
        zone: u64,
        fixed_offset_seconds: i32,
        local: NativePlatformDateTime,
        resolution: *mut NativePlatformTemporalResolution,
        outcome: *mut u32,
    ) -> NativePlatformStatus {
        let Some(resolution_region) = MemoryRegion::write(resolution) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(outcome_region) = MemoryRegion::write(outcome) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if resolution_region.overlaps(outcome_region) {
            return NativePlatformStatus::INVALID_INPUT;
        }

        unsafe {
            resolution.write(NativePlatformTemporalResolution::default());
            outcome.write(bray_temporal_resolve(
                zone,
                fixed_offset_seconds,
                local,
                resolution,
            ));
        }

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_time_parse(
        kind: u32,
        text: NativePlatformText,
        value: *mut NativePlatformTemporalValue,
        invalid_offset: *mut u64,
        outcome: *mut u32,
    ) -> NativePlatformStatus {
        let Ok(length) = usize::try_from(text.length()) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(text_region) = MemoryRegion::read(text.address(), length) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(value_region) = MemoryRegion::write(value) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(offset_region) = MemoryRegion::write(invalid_offset) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(outcome_region) = MemoryRegion::write(outcome) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if !disjoint(&[text_region, value_region, offset_region, outcome_region]) {
            return NativePlatformStatus::INVALID_INPUT;
        }

        unsafe {
            value.write(NativePlatformTemporalValue::default());
            invalid_offset.write(0);
            outcome.write(bray_temporal_parse(
                kind,
                text.address(),
                text.length(),
                value,
                invalid_offset,
            ));
        }

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_time_format(
        kind: u32,
        value: NativePlatformTemporalValue,
        destination: *mut u8,
        capacity: u64,
        written_or_required: *mut u64,
        outcome: *mut u32,
    ) -> NativePlatformStatus {
        let Some(regions) = variable_output_regions(destination, capacity, written_or_required, outcome)
        else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if !disjoint(&regions) {
            return NativePlatformStatus::INVALID_INPUT;
        }

        unsafe {
            written_or_required.write(0);
            outcome.write(bray_temporal_format(
                kind,
                value,
                destination,
                capacity,
                written_or_required,
            ));
        }

        NativePlatformStatus::SUCCESS
    }
}

fn variable_output_regions(
    destination: *mut u8,
    capacity: u64,
    written_or_required: *mut u64,
    outcome: *mut u32,
) -> Option<Vec<MemoryRegion>> {
    let capacity = usize::try_from(capacity).ok()?;
    let destination = MemoryRegion::read(destination, capacity)?;
    let written_or_required = MemoryRegion::write(written_or_required)?;
    let outcome = MemoryRegion::write(outcome)?;

    Some(vec![destination, written_or_required, outcome])
}

fn provider_status(outcome: u32) -> NativePlatformStatus {
    match outcome {
        0 => NativePlatformStatus::SUCCESS,
        1 => NativePlatformStatus::UNSUPPORTED,
        2 => NativePlatformStatus::INVALID_INPUT,
        3..=7 => NativePlatformStatus::OTHER,
        _ => NativePlatformStatus::OTHER,
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_interface::{
        NativePlatformDateTime, NativePlatformStatus, NativePlatformTemporalObservation,
        NativePlatformTemporalResolution, NativePlatformTemporalValue, NativePlatformText,
    };

    use super::{
        bray_platform_time_date_add, bray_platform_time_date_validate, bray_platform_time_format,
        bray_platform_time_observe, bray_platform_time_parse, bray_platform_time_resolve,
        bray_platform_time_zone_close, bray_platform_time_zone_load, bray_platform_time_zone_name,
    };

    #[test]
    fn civil_dates_validate_and_apply_explicit_month_adjustment() {
        let mut outcome = 0;

        assert_eq!(
            bray_platform_time_date_validate(2000, 2, 29, &mut outcome),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 0);

        assert_eq!(
            bray_platform_time_date_validate(1900, 2, 29, &mut outcome),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 2);

        assert_eq!(
            bray_platform_time_date_validate(65_536, 1, 1, &mut outcome),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 4);

        let value = date_time(2024, 1, 31, 0, 0, 0, 0);
        let mut result = NativePlatformDateTime::default();

        assert_eq!(
            bray_platform_time_date_add(value, 0, 1, 0, 0, &mut result, &mut outcome),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 2);

        assert_eq!(
            bray_platform_time_date_add(value, 0, 1, 0, 1, &mut result, &mut outcome),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 0);
        assert_eq!(result, date_time(2024, 2, 29, 0, 0, 0, 0));

        assert_eq!(
            bray_platform_time_date_add(
                date_time(32_767, 12, 31, 0, 0, 0, 0),
                1,
                0,
                0,
                0,
                &mut result,
                &mut outcome,
            ),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 4);
    }

    #[test]
    fn named_zones_use_pinned_aliases_and_classify_transitions() {
        let zone = load_zone("US/Eastern");
        let mut required = 0;
        let mut outcome = 0;

        assert_eq!(
            bray_platform_time_zone_name(
                zone,
                std::ptr::null_mut(),
                0,
                &mut required,
                &mut outcome,
            ),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 6);

        let mut name = vec![0_u8; required as usize];

        assert_eq!(
            bray_platform_time_zone_name(
                zone,
                name.as_mut_ptr(),
                name.len() as u64,
                &mut required,
                &mut outcome,
            ),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 0);
        assert_eq!(name, b"America/New_York");

        let (observation, abbreviation) = observe(zone, 1_710_054_000);

        assert_eq!(observation.local(), date_time(2024, 3, 10, 3, 0, 0, 0));
        assert_eq!(observation.offset_seconds(), -14_400);
        assert!(observation.is_daylight_saving());
        assert_eq!(abbreviation, b"EDT");

        let (summer, abbreviation) = observe(zone, 1_719_835_200);

        assert_eq!(summer.local(), date_time(2024, 7, 1, 8, 0, 0, 0));
        assert_eq!(summer.offset_seconds(), -14_400);
        assert!(summer.is_daylight_saving());
        assert_eq!(abbreviation, b"EDT");

        let repeated = resolve(zone, date_time(2024, 11, 3, 1, 30, 0, 0));

        assert_eq!(repeated.kind(), 1);
        assert_eq!(repeated.first_seconds(), 1_730_611_800);
        assert_eq!(repeated.second_seconds(), 1_730_615_400);

        let skipped = resolve(zone, date_time(2024, 3, 10, 2, 30, 0, 0));

        assert_eq!(skipped.kind(), 2);
        assert_eq!(skipped.first_seconds(), 1_710_053_999);
        assert_eq!(skipped.first_nanoseconds(), 999_999_999);
        assert_eq!(skipped.second_seconds(), 1_710_054_000);
        assert_eq!(skipped.second_nanoseconds(), 0);

        assert_eq!(
            bray_platform_time_zone_close(zone),
            NativePlatformStatus::SUCCESS
        );
    }

    #[test]
    fn strict_timestamp_text_round_trips_without_named_zone_data() {
        let source = b"2024-02-29T12:34:56.125+05:30";
        let mut value = NativePlatformTemporalValue::default();
        let mut invalid_offset = 0;
        let mut outcome = 0;

        assert_eq!(
            bray_platform_time_parse(
                3,
                NativePlatformText::new(source.as_ptr(), source.len() as u64),
                &mut value,
                &mut invalid_offset,
                &mut outcome,
            ),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 0);
        assert_eq!(invalid_offset, source.len() as u64);
        assert_eq!(value.offset_seconds(), 19_800);

        let mut required = 0;

        assert_eq!(
            bray_platform_time_format(
                3,
                value,
                std::ptr::null_mut(),
                0,
                &mut required,
                &mut outcome,
            ),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 6);

        let mut formatted = vec![0_u8; required as usize];

        assert_eq!(
            bray_platform_time_format(
                3,
                value,
                formatted.as_mut_ptr(),
                formatted.len() as u64,
                &mut required,
                &mut outcome,
            ),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 0);
        assert_eq!(formatted, source);

        let expanded = b"+010000-01-02";

        assert_eq!(
            bray_platform_time_parse(
                0,
                NativePlatformText::new(expanded.as_ptr(), expanded.len() as u64),
                &mut value,
                &mut invalid_offset,
                &mut outcome,
            ),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 0);

        assert_eq!(
            bray_platform_time_format(
                0,
                value,
                std::ptr::null_mut(),
                0,
                &mut required,
                &mut outcome,
            ),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 6);

        formatted.resize(required as usize, 0);

        assert_eq!(
            bray_platform_time_format(
                0,
                value,
                formatted.as_mut_ptr(),
                formatted.len() as u64,
                &mut required,
                &mut outcome,
            ),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 0);
        assert_eq!(formatted, expanded);

        let mut observation = NativePlatformTemporalObservation::default();

        assert_eq!(
            bray_platform_time_observe(
                0,
                0,
                i64::MAX,
                0,
                &mut observation,
                std::ptr::null_mut(),
                0,
                &mut required,
                &mut outcome,
            ),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 4);
    }

    fn load_zone(name: &str) -> u64 {
        let mut handle = 0;
        let mut outcome = 0;

        assert_eq!(
            bray_platform_time_zone_load(
                NativePlatformText::new(name.as_ptr(), name.len() as u64),
                &mut handle,
                &mut outcome,
            ),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 0);
        assert_ne!(handle, 0);

        handle
    }

    fn resolve(zone: u64, local: NativePlatformDateTime) -> NativePlatformTemporalResolution {
        let mut resolution = NativePlatformTemporalResolution::default();
        let mut outcome = 0;

        assert_eq!(
            bray_platform_time_resolve(zone, 0, local, &mut resolution, &mut outcome),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 0);

        resolution
    }

    fn observe(zone: u64, seconds: i64) -> (NativePlatformTemporalObservation, Vec<u8>) {
        let mut observation = NativePlatformTemporalObservation::default();
        let mut required = 0;
        let mut outcome = 0;

        assert_eq!(
            bray_platform_time_observe(
                zone,
                0,
                seconds,
                0,
                &mut observation,
                std::ptr::null_mut(),
                0,
                &mut required,
                &mut outcome,
            ),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 6);

        let mut abbreviation = vec![0_u8; required as usize];

        assert_eq!(
            bray_platform_time_observe(
                zone,
                0,
                seconds,
                0,
                &mut observation,
                abbreviation.as_mut_ptr(),
                abbreviation.len() as u64,
                &mut required,
                &mut outcome,
            ),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(outcome, 0);

        abbreviation.truncate(required as usize);

        (observation, abbreviation)
    }

    const fn date_time(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        nanosecond: u32,
    ) -> NativePlatformDateTime {
        NativePlatformDateTime::new(year, month, day, hour, minute, second, nanosecond)
    }
}
