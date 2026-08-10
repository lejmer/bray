//! Temporal platform ABI exports isolated from unrelated platform mechanisms.

#![deny(unsafe_code)]

mod temporal;

pub use temporal::{
    bray_platform_time_date_add, bray_platform_time_date_validate, bray_platform_time_format,
    bray_platform_time_observe, bray_platform_time_parse, bray_platform_time_resolve,
    bray_platform_time_zone_close, bray_platform_time_zone_load, bray_platform_time_zone_local,
    bray_platform_time_zone_name, bray_platform_time_zone_retain,
};
