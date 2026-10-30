#include <stdlib.h>

#include "native-execution/provider-host.h"

static static_host_entry no_resident_statics(size_t ordinal)
{
    (void)ordinal;
    abort();
}

// These C test hosts are resident final products with no Bray user statics.
const product_host_descriptor bray_linked_product_host =
{
    2, 0, {0x55, 0x55, 0x35}, no_resident_statics, 0, NULL, NULL, NULL
};
