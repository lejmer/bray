#ifndef BRAY_STATIC_STORAGE_HOST_H
#define BRAY_STATIC_STORAGE_HOST_H

#include <stddef.h>
#include <stdint.h>

#include "../runtime-cleanup-abi.h"

enum product_host_operation
{
    PRODUCT_HOST_FORM = 0,
    PRODUCT_HOST_ACQUIRE_ENTRY = 1,
    PRODUCT_HOST_RELEASE_ENTRY = 2,
    PRODUCT_HOST_ACQUIRE_EXTERNAL = 3,
    PRODUCT_HOST_RELEASE_EXTERNAL = 4,
    PRODUCT_HOST_CLOSE = 5,
    PRODUCT_HOST_OBSERVE = 6,
    PRODUCT_HOST_ATTACH_CURRENT_THREAD = 9,
    PRODUCT_HOST_DETACH_CURRENT_THREAD = 10,
};

typedef struct
{
    uintptr_t context;
    const void *callbacks;
    uintptr_t retention_context;
    const void *retention_callbacks;
} CleanupCapacityBinding;

extern uint32_t bray_runtime_cleanup_capacity_domain_formation(CleanupCapacityBinding *binding);
extern void bray_runtime_cleanup_capacity_domain_release(CleanupCapacityBinding *binding);

typedef ProductHostObservation (*product_host_control)(
    uint32_t operation,
    const CleanupCapacityBinding *capacity
);

#endif
