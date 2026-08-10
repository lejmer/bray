use bray_runtime_interface::{
    NativePlatformDateTime, NativePlatformStatus, NativePlatformTemporalObservation,
    NativePlatformTemporalResolution, NativePlatformTemporalValue, NativePlatformText,
};
use bray_platform_abi_support::{MemoryRegion, disjoint, native_platform_export};

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

