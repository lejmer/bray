#include "static_storage_host.h"

static CleanupCapacityBinding capacity = {0};

#ifndef BRAY_PRODUCT_HOST_CONTROL
#error BRAY_PRODUCT_HOST_CONTROL must name the compiler-generated control symbol
#endif

extern ProductHostObservation BRAY_PRODUCT_HOST_CONTROL(
    uint32_t operation,
    const CleanupCapacityBinding *capacity
);

int main(void)
{
    if (bray_runtime_cleanup_capacity_domain_formation(&capacity) != 0)
        return 3;

    ProductHostObservation formed = BRAY_PRODUCT_HOST_CONTROL(PRODUCT_HOST_FORM, &capacity);

    if (formed.status != 0 || formed.state != 1 || formed.initialized_statics < 4)
        return 1;

    ProductHostObservation closed = BRAY_PRODUCT_HOST_CONTROL(PRODUCT_HOST_CLOSE, &capacity);

    if (
        closed.status != 4 ||
        closed.state != 3 ||
        closed.cleaned_statics != formed.initialized_statics ||
        closed.cleanup_incidents != 1
    )
        return 2;

    bray_runtime_cleanup_capacity_domain_release(&capacity);

    return 0;
}
