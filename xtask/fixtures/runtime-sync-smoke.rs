#[repr(transparent)]
#[derive(Clone, Copy, Eq, PartialEq)]
struct RunState(u32);

impl RunState {
    const COMPLETED: Self = Self(0);
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RunOutcome {
    state: RunState,
    payload: usize,
}

unsafe extern "C" {
    safe fn bray_runtime_synchronous_root_execution(
        callback: extern "C" fn(usize),
        destination: usize,
    ) -> RunOutcome;
}

extern "C" fn root(destination: usize) {
    assert_eq!(destination, 17);
}

fn main() {
    let outcome = bray_runtime_synchronous_root_execution(root, 17);

    assert!(outcome.state == RunState::COMPLETED);
    assert_eq!(outcome.payload, 17);
}
