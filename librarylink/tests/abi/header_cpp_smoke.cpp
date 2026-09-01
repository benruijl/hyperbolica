#include "hyperbolica_librarylink.h"

#include <type_traits>

static_assert(WolframLibraryVersion == 6);
static_assert(std::is_trivially_copyable_v<MArgument>);

int main() {
    return 0;
}

