use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

fn main() {
    if std::env::args().nth(1).as_deref() == Some("descendant") {
        std::fs::write("ready", b"ready").expect("announce descendant");

        while !Path::new("release").exists() {
            std::thread::sleep(Duration::from_millis(5));
        }

        std::fs::write("finished", b"finished").expect("finish descendant");

        return;
    }

    let mode = std::fs::read_to_string("scenario").expect("fixture scenario");

    if mode != "abort_early" {
        std::io::stdin().read_exact(&mut [0_u8; 1]).expect("native startup input");
    }

    if mode == "descendant" {
        let _child = std::process::Command::new(std::env::current_exe().expect("fixture executable"))
            .arg("descendant")
            .stdin(std::process::Stdio::null())
            .spawn()
            .expect("start descendant holding output handles");

        while !Path::new("ready").exists() {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    if mode == "flood" {
        let out = std::thread::spawn(|| {
            std::io::stdout().write_all(&vec![b'o'; 131072]).expect("flood stdout");
        });

        let err = std::thread::spawn(|| {
            std::io::stderr().write_all(&vec![b'e'; 131072]).expect("flood stderr");
        });

        out.join().expect("stdout writer completes");
        err.join().expect("stderr writer completes");
        std::process::exit(37);
    }

    std::io::stdout().write_all(b"startup stdout\n").expect("startup stdout");
    std::io::stderr().write_all(b"fatal host startup\n").expect("fatal stderr");

    if mode == "abort" || mode == "abort_early" {
        std::process::abort();
    }

    if mode == "malformed" {
        std::fs::write(std::env::var_os("BRAY_TEST_RESULT_PATH").expect("result path"), b"not a host result")
            .expect("malformed terminal result");
    }

    if mode == "hang" {
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    std::process::exit(if mode == "missing" || mode == "malformed" { 0 } else { 37 });
}
