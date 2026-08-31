#include "hyperbolica/c_abi.h"

#include <type_traits>

static_assert(HF_SCHEMA_VERSION == 2);
static_assert(std::is_same_v<decltype(&hf_free_string), void (*)(char *)>);
static_assert(
    std::is_same_v<decltype(&hf_partial_fractions), char *(*)(const char *)>);
static_assert(std::is_same_v<decltype(&hf_linear_factors),
                             char *(*)(const char *)>);
static_assert(
    std::is_same_v<decltype(&hf_find_lr_orders), char *(*)(const char *)>);
static_assert(std::is_same_v<decltype(&hf_factor_table),
                             char *(*)(const char *)>);
static_assert(std::is_same_v<decltype(&hf_find_lr_orders_scan),
                             char *(*)(const char *)>);
static_assert(
    std::is_same_v<decltype(&hf_hyperflint_sym), char *(*)(const char *)>);
static_assert(
    std::is_same_v<decltype(&hf_version_string), const char *(*)()>);

int main() { return 0; }
