// Included in the pinned LLVM assembly writer translation unit.
#include "telemetry.h"

namespace
{

class DefinitionSizeStream final : public llvm::raw_ostream
{
public:
    DefinitionSizeStream() : llvm::raw_ostream(true) {}

private:
    void write_impl(const char*, std::size_t size) override
    {
        bytes += size;
    }

    std::uint64_t current_pos() const override
    {
        return bytes;
    }

    std::uint64_t bytes = 0;
};

}

void bray::lld::definition_sizes(
    const llvm::Module& module,
    llvm::function_ref<void(const llvm::GlobalValue&, std::uint64_t)> record
)
{
    DefinitionSizeStream output;
    llvm::formatted_raw_ostream formatted(output);
    llvm::SlotTracker slots(&module, true);
    AssemblyWriter writer(formatted, slots, &module, nullptr, false);

    for (const llvm::GlobalValue& value : module.global_values())
    {
        if (value.isDeclaration())
            continue;

        const std::uint64_t before = output.tell();

        if (const auto* global = llvm::dyn_cast<llvm::GlobalVariable>(&value))
            writer.printGlobal(global);
        else if (const auto* function = llvm::dyn_cast<llvm::Function>(&value))
            writer.printFunction(function);
        else if (const auto* alias = llvm::dyn_cast<llvm::GlobalAlias>(&value))
            writer.printAlias(alias);
        else if (const auto* ifunc = llvm::dyn_cast<llvm::GlobalIFunc>(&value))
            writer.printIFunc(ifunc);
        else
            llvm_unreachable("Unknown global definition to measure");

        formatted.flush();
        record(value, output.tell() - before);
    }
}
