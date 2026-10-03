#include "telemetry.h"

#include "llvm/ADT/DenseMap.h"
#include "llvm/AsmParser/Parser.h"
#include "llvm/IR/LLVMContext.h"
#include "llvm/IR/Module.h"
#include "llvm/Support/SourceMgr.h"
#include "llvm/Support/raw_ostream.h"

#include <cstdlib>
#include <string>

int main()
{
    std::string source = R"(
%record = type { i32, ptr }
$group = comdat any
@data = global %record { i32 7, ptr @function }, !annotation !0
@alias = alias i32 (i32), ptr @function
@indirect = ifunc i32 (i32), ptr @resolver
declare i32 @external(i32)
define i32 @function(i32 %value) comdat($group) {
entry:
  %result = call i32 @external(i32 %value), !annotation !0
  ret i32 %result
}
define ptr @resolver() {
  ret ptr @function
}
!0 = !{!"shared"}
)";

    for (unsigned index = 1; index <= 128; ++index)
    {
        const std::string suffix = std::to_string(index);

        source += "@data" + suffix + " = global i32 " + suffix + "\n";
        source += "define i32 @function" + suffix + "(i32 %value) #" + suffix;
        source += " {\n  %result = add i32 %value, " + suffix;
        source += ", !annotation !" + suffix + "\n  ret i32 %result\n}\n";
        source += "attributes #" + suffix + " = { \"identity\"=\"" + suffix + "\" }\n";
        source += "!" + suffix + " = !{!\"" + suffix + "\"}\n";
    }

    llvm::LLVMContext context;
    llvm::SMDiagnostic diagnostic;
    const auto module = llvm::parseAssemblyString(source, diagnostic, context);

    if (!module)
    {
        diagnostic.print("definition-sizes-test", llvm::errs());
        return 1;
    }

    llvm::DenseMap<llvm::GlobalValue::GUID, std::uint64_t> actual;

    bray::lld::definition_sizes(*module, [&actual](
        const llvm::GlobalValue& value,
        std::uint64_t bytes
    ) {
        actual.try_emplace(value.getGUID(), bytes);
    });

    std::size_t definitions = 0;

    for (const llvm::GlobalValue& value : module->global_values())
    {
        if (value.isDeclaration())
            continue;

        std::string representation;
        llvm::raw_string_ostream output(representation);

        value.print(output);
        output.flush();
        ++definitions;

        if (actual.lookup(value.getGUID()) != representation.size())
        {
            llvm::errs() << "Definition byte count differs for " << value.getName() << '\n';
            return 1;
        }
    }

    if (definitions != actual.size())
        return 1;

    return 0;
}
