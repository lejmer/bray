#ifndef BRAY_PROVIDER_HOST_H
#define BRAY_PROVIDER_HOST_H

#include <stdatomic.h>
#include "static-storage-host.h"

typedef struct
{
    void (*retain)(uintptr_t context);
    void (*release)(uintptr_t context);
    size_t (*references)(uintptr_t context);
} provider_callbacks;

typedef struct
{
    uintptr_t context;
    const provider_callbacks* callbacks;
} provider_reference;

typedef struct
{
    uintptr_t descriptor;
    uint64_t epoch;
} provider_handle;

typedef struct
{
    product_host_observation observation;
    provider_handle handle;
} provider_observation;

typedef struct
{
    product_host_observation observation;
    provider_reference provider;
} provider_retirement;

typedef struct
{
    void* host;
    void* synchronization;
    void* pool;
    const provider_callbacks* callbacks;
    void* consumer;
    uint32_t (*control)(uintptr_t, uint64_t, uint32_t, provider_observation*);
    uint32_t (*retire)(uintptr_t, uint64_t, provider_retirement*);
    uint32_t (*notify)(uintptr_t, uintptr_t, const provider_callbacks*);
} provider_domain;

typedef struct
{
    uint32_t version;
    uint32_t services;
    const provider_domain* domain;
    provider_reference provider;
    const provider_handle* dependencies;
    size_t dependency_count;
} provider_binding;

typedef struct
{
    uint32_t (*form)(uintptr_t, const provider_binding*, provider_observation*);
    uint32_t (*root)(uintptr_t, provider_reference*);
} provider_operations;

typedef uintptr_t (*static_access)(void);
typedef void (*static_cleanup)(uint32_t abandoned, void* outcome);
typedef void (*static_transition)(void);
typedef void (*static_admission)(uint32_t operation, void* outcome);
typedef uint32_t (*static_finalizer_start)(uintptr_t destination, void* outcome);
typedef uint32_t (*static_finalizer_resolve)(uintptr_t completion, uintptr_t incident, void* outcome);

typedef struct
{
    uint32_t execution;
    static_admission admission;
    static_finalizer_start start;
    static_finalizer_resolve resolve;
} static_finalizer;

typedef struct
{
    uint8_t bytes[32];
} static_identity;

typedef static_identity (*static_dependency)(size_t index);

typedef struct static_host_entry
{
    uint32_t abi_version;
    uint32_t duration;
    static_identity identity;
    uint64_t order;
    uintptr_t storage;
    static_access access;
    static_transition prepare;
    static_finalizer finalizer;
    static_cleanup destroy;
    static_transition detach;
    static_dependency dependency;
    size_t dependency_count;
} static_host_entry;

typedef static_host_entry (*static_host_lookup)(size_t index);

typedef struct
{
    uint32_t abi_version;
    uint32_t reserved;
    uint8_t identity[32];
    static_host_lookup static_entry;
    size_t static_count;
    void* execution;
    atomic_uintptr_t* activation;
    const provider_operations* provider;
} product_host_descriptor;

extern const product_host_descriptor bray_linked_product_host;

typedef struct provider_host_owner
{
    void* library;
    atomic_size_t references;
    uintptr_t descriptor;
    unsigned closes;
    void (*observe)(struct provider_host_owner*);
} provider_host_owner;

extern provider_domain provider_host_domain;
extern const provider_callbacks provider_host_callbacks;

void* provider_host_open(const char* path);
void* provider_host_symbol(void* library, const char* name);
void provider_host_initialize(void);
void provider_host_retain(uintptr_t context);
void provider_host_release(uintptr_t context);
size_t provider_host_references(uintptr_t context);

#endif
