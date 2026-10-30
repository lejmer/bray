#include <stdlib.h>

#include "provider-host.h"

#if defined(_WIN32)
#define PROVIDER_EXPORT __declspec(dllexport)
#else
#define PROVIDER_EXPORT __attribute__((visibility("default")))
#endif

extern uint32_t bray_runtime_product_provider_formation(uintptr_t, const provider_binding*, provider_observation*);
extern uint32_t bray_runtime_product_provider_root(uintptr_t, provider_reference*);
extern uint32_t bray_runtime_provider_domain_access(provider_domain*);

static static_host_entry no_statics(size_t index)
{
    (void)index;
    abort();
}

static atomic_uintptr_t activation = ATOMIC_VAR_INIT(0);
static const provider_operations operations =
{
    bray_runtime_product_provider_formation,
    bray_runtime_product_provider_root
};

PROVIDER_EXPORT const product_host_descriptor bray_linked_product_host =
{
    2, 0, {0x55, 0x55, 0x36}, no_statics, 0, NULL, &activation, &operations
};

PROVIDER_EXPORT uint32_t bray_fixture_provider_domain(provider_domain* output)
{
    return bray_runtime_provider_domain_access(output);
}
