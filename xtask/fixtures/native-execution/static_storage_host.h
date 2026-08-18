#ifndef BRAY_STATIC_STORAGE_HOST_H
#define BRAY_STATIC_STORAGE_HOST_H

#include <stddef.h>
#include <stdint.h>

enum product_host_operation {
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

typedef struct {
    uint32_t status;
    uint32_t state;
    size_t active_entries;
    size_t external_roots;
    size_t thread_attachments;
    size_t initialized_statics;
    size_t cleaned_statics;
    size_t cleanup_incidents;
    uint8_t last_incident[32];
} product_host_observation;

typedef product_host_observation (*product_host_control)(uint32_t operation);

#endif
