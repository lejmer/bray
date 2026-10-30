#include <stdlib.h>
#include "provider-host.h"

#if defined(_WIN32)
#include <windows.h>
#else
#include <dlfcn.h>
#endif

extern uint32_t bray_runtime_provider_domain_access(provider_domain*);

provider_domain provider_host_domain;

const provider_callbacks provider_host_callbacks =
{
    provider_host_retain,
    provider_host_release,
    provider_host_references
};

void* provider_host_open(const char* path)
{
#if defined(_WIN32)
    return LoadLibraryA(path);
#else
    return dlopen(path, RTLD_NOW | RTLD_LOCAL);
#endif
}

void* provider_host_symbol(void* library, const char* name)
{
#if defined(_WIN32)
    return (void*)GetProcAddress((HMODULE)library, name);
#else
    return dlsym(library, name);
#endif
}

void provider_host_initialize(void)
{
    if (bray_runtime_provider_domain_access(&provider_host_domain) != 0)
    {
        abort();
    }
}

void provider_host_retain(uintptr_t context)
{
    provider_host_owner* owner = (provider_host_owner*)context;
    size_t previous = atomic_fetch_add_explicit(&owner->references, 1, memory_order_acq_rel);

    if (previous == 0 || previous == SIZE_MAX)
    {
        abort();
    }
}

size_t provider_host_references(uintptr_t context)
{
    provider_host_owner* owner = (provider_host_owner*)context;

    if (owner->observe != NULL)
    {
        owner->observe(owner);
    }

    return atomic_load_explicit(&owner->references, memory_order_acquire);
}

void provider_host_release(uintptr_t context)
{
    provider_host_owner* owner = (provider_host_owner*)context;
    size_t previous = atomic_fetch_sub_explicit(&owner->references, 1, memory_order_acq_rel);

    if (previous == 0)
    {
        abort();
    }

    if (previous == 1)
    {
#if defined(_WIN32)
        if (!FreeLibrary((HMODULE)owner->library))
#else
        if (dlclose(owner->library) != 0)
#endif
        {
            abort();
        }

        owner->closes += 1;
    }
    else if (owner->descriptor != 0)
    {
        (void)provider_host_domain.notify(owner->descriptor, context, &provider_host_callbacks);
    }
}
