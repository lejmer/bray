#include "static_storage_host.h"

#ifndef BRAY_PRODUCT_HOST_CONTROL
#error BRAY_PRODUCT_HOST_CONTROL must name the compiler-generated control symbol
#endif

extern ProductHostObservation BRAY_PRODUCT_HOST_CONTROL(uint32_t operation);

int main(void)
{
    ProductHostObservation formed = BRAY_PRODUCT_HOST_CONTROL(PRODUCT_HOST_FORM);

    if (formed.status != 0 || formed.state != 1 || formed.initialized_statics < 4)
        return 1;

    ProductHostObservation closed = BRAY_PRODUCT_HOST_CONTROL(PRODUCT_HOST_CLOSE);

    if (
        closed.status != 4 ||
        closed.state != 3 ||
        closed.cleaned_statics != formed.initialized_statics ||
        closed.cleanup_incidents != 1
    )
        return 2;

    return 0;
}
