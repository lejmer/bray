include!("panic_report.rs");

#[repr(transparent)]
#[derive(Clone, Copy, Eq, PartialEq)]
struct RunState(u32);

impl RunState {
    const COMPLETED: Self = Self(0);
}

#[repr(C)]
struct RunOutcome {
    state: RunState,
    payload: usize,
    report: PanicReport,
}

unsafe extern "C" {
    safe fn bray_runtime_synchronous_root_execution(
        callback: extern "C" fn(usize, *mut RunOutcome, *mut ()),
        destination: usize,
        outcome: &mut RunOutcome,
    ) -> u32;
}

extern "C" fn root(destination: usize, outcome: *mut RunOutcome, _: *mut ()) {
    assert_eq!(destination, 17);

    unsafe {
        outcome.write(RunOutcome {
            state: RunState::COMPLETED,
            payload: destination,
            report: PanicReport::empty(),
        });
    }
}

fn main() {
    let mut outcome = RunOutcome { state: RunState(3), payload: 4, report: PanicReport::empty() };

    assert_eq!(bray_runtime_synchronous_root_execution(root, 17, &mut outcome), 0);

    assert!(outcome.state == RunState::COMPLETED);
    assert_eq!(outcome.payload, 17);

    for _ in 0..1_000 {
        assert_eq!(bray_runtime_synchronous_root_execution(root, 17, &mut outcome), 0);
    }

    let begin = std::time::Instant::now();

    for _ in 0..100_000 {
        assert_eq!(bray_runtime_synchronous_root_execution(root, 17, &mut outcome), 0);
        assert!(outcome.state == RunState::COMPLETED);
        assert_eq!(outcome.payload, 17);
    }

    println!("synchronous_entry iterations=100000 elapsed_seconds={:.9}", begin.elapsed().as_secs_f64());
}
