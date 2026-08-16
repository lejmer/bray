#![no_std]

pub fn accumulate(seed: u64, mut count: u64) -> u64 {
    let mut value = seed;

    while count > 0 {
        count -= 1;
        value += count;
    }

    value
}
