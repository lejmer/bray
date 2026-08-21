#include "telemetry.h"

#include "lld/Common/Driver.h"
#include "llvm/Support/raw_ostream.h"

#if defined(_WIN32)
LLD_HAS_DRIVER(coff)
#elif defined(__APPLE__)
LLD_HAS_DRIVER(macho)
#else
LLD_HAS_DRIVER(elf)
#endif

int main(int argc, char** argv)
{
#if defined(_WIN32)
    const lld::DriverDef driver{lld::WinLink, &lld::coff::link};
#elif defined(__APPLE__)
    const lld::DriverDef driver{lld::Darwin, &lld::macho::link};
#else
    const lld::DriverDef driver{lld::Gnu, &lld::elf::link};
#endif
    const llvm::ArrayRef<const char*> arguments(argv, argv + argc);
    const llvm::ArrayRef drivers(&driver, 1);
    const lld::Result result = lld::lldMain(
        arguments, llvm::outs(), llvm::errs(), drivers
    );

    if (result.retCode != 0)
    {
        bray::lld::abandon_telemetry();

        return result.retCode;
    }

    if (!bray::lld::finish_telemetry())
        return 86;

    return result.retCode;
}
