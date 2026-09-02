#include "hyperbolica_librarylink.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L
_Static_assert(sizeof(MArgument) == sizeof(void *),
               "MArgument must occupy one pointer slot");
_Static_assert(sizeof(mint) == sizeof(void *),
               "mint must match the host pointer width");
#endif

typedef int (*json_operation)(WolframLibraryData, mint, MArgument *, MArgument);

struct operation_case {
    const char *name;
    const char *op;
    json_operation function;
};

struct strategy_case {
    const char *name;
    const char *request;
    const char *strategy;
};

struct success_case {
    const char *name;
    const char *op;
    json_operation function;
    const char *request;
    const char *required_field;
};

static unsigned int disown_count = 0;

static void test_disown(char *value) {
    ++disown_count;
    free(value);
}

static char *copy_text(const char *value) {
    const size_t size = strlen(value) + 1;
    char *copy = (char *)malloc(size);
    if (copy != NULL) {
        memcpy(copy, value, size);
    }
    return copy;
}

static int call_json(WolframLibraryData lib_data,
                     json_operation function,
                     const char *request,
                     char **response_copy) {
    char *input = copy_text(request);
    char *input_slot = input;
    char *output_slot = NULL;
    MArgument argument;
    MArgument result;
    int code;

    if (input == NULL) {
        return 0;
    }
    argument.utf8string = &input_slot;
    result.utf8string = &output_slot;
    code = function(lib_data, 1, &argument, result);
    if (code != LIBRARY_NO_ERROR || output_slot == NULL) {
        return 0;
    }
    *response_copy = copy_text(output_slot);
    return *response_copy != NULL;
}

static int has_field(const char *json, const char *field) {
    return strstr(json, field) != NULL;
}

int main(void) {
    static const struct operation_case operations[] = {
        {"hf_find_lr_orders", "find_lr_orders", hf_find_lr_orders},
        {"hf_factor_table", "factor_table", hf_factor_table},
        {"hf_find_lr_orders_scan", "find_lr_orders_scan", hf_find_lr_orders_scan},
        {"hf_hyperflint_sym", "hyperflint", hf_hyperflint_sym},
    };
    struct st_WolframLibraryData callbacks = {test_disown};
    size_t index;
    int failures = 0;
    char *version_slot = NULL;
    char *version_copy = NULL;
    MArgument version_result;
    mint clear_value = -1;
    MArgument clear_result;

    if (WolframLibrary_getVersion() != WolframLibraryVersion) {
        fputs("LibraryLink smoke: ABI version mismatch\n", stderr);
        ++failures;
    }
    if (WolframLibrary_initialize(&callbacks) != LIBRARY_NO_ERROR) {
        fputs("LibraryLink smoke: initialization failed\n", stderr);
        return 1;
    }

    version_result.utf8string = &version_slot;
    if (hf_version(&callbacks, 0, NULL, version_result) != LIBRARY_NO_ERROR ||
        version_slot == NULL || version_slot[0] == '\0') {
        fputs("LibraryLink smoke: invalid hf_version result\n", stderr);
        ++failures;
    } else {
        version_copy = copy_text(version_slot);
        if (version_copy == NULL) {
            ++failures;
        }
    }

    for (index = 0; index < sizeof(operations) / sizeof(operations[0]); ++index) {
        char *response = NULL;
        char op_field[96];
        const unsigned int before = disown_count;
        const int written = snprintf(op_field, sizeof(op_field),
                                     "\"op\":\"%s\"", operations[index].op);

        if (!call_json(&callbacks, operations[index].function, "{not-json}", &response) ||
            written <= 0 || (size_t)written >= sizeof(op_field) ||
            !has_field(response, op_field) ||
            !has_field(response, "\"schema_version\":2") ||
            !has_field(response, "\"hf_version\":") ||
            !has_field(response, "\"error\":") ||
            disown_count != before + 1) {
            fprintf(stderr, "LibraryLink smoke: %s marshalling failed\n",
                    operations[index].name);
            ++failures;
        }
        free(response);
    }

    if (getenv("HYPERBOLICA_LIBRARYLINK_LICENSED_TEST") != NULL) {
        static const struct strategy_case strategies[] = {
            {"lr_db1_lungo",
             "{\"op\":\"find_lr_orders\",\"xvars\":[\"x1\",\"x2\"],"
             "\"polys\":[\"x1\",\"x2\",\"x1+x2\"]}",
             "LR_NoOpt"},
            {"lr_db1_espresso",
             "{\"op\":\"find_lr_orders\",\"xvars\":[\"x1\",\"x2\"],"
             "\"polys\":[\"x1\",\"x2\",\"x1+x2\"],"
             "\"method_lr_hint\":\"Espresso\"}",
             "LR_NoOpt"},
            {"lr_db2_lungo",
             "{\"op\":\"find_lr_orders\",\"xvars\":[\"x1\",\"x2\"],"
             "\"polys\":[\"x1\",\"x2\",\"x1+x2\"],"
             "\"algebraic_letters\":true}",
             "LR_OptOrdered"},
            {"lr_db2_espresso",
             "{\"op\":\"find_lr_orders\",\"xvars\":[\"x1\",\"x2\"],"
             "\"polys\":[\"x1\",\"x2\",\"x1+x2\"],"
             "\"algebraic_letters\":true,\"method_lr_hint\":\"Espresso\"}",
             "LR_OptOrdered"},
            {"nolr_lungo",
             "{\"op\":\"find_lr_orders\",\"xvars\":[\"x1\",\"x2\"],"
             "\"polys\":[\"1+x1^2+x2^2\"]}",
             "Fubini_Lungo"},
            {"nolr_espresso",
             "{\"op\":\"find_lr_orders\",\"xvars\":[\"x1\",\"x2\"],"
             "\"polys\":[\"1+x1^2+x2^2\"],"
             "\"method_lr_hint\":\"Espresso\"}",
             "Fubini_Espresso"},
        };
        for (index = 0; index < sizeof(strategies) / sizeof(strategies[0]); ++index) {
            char *response = NULL;
            char expected[96];
            const int written = snprintf(expected, sizeof(expected),
                                         "\"strategy\":\"%s\"",
                                         strategies[index].strategy);
            if (!call_json(&callbacks, hf_find_lr_orders,
                           strategies[index].request, &response) ||
                written <= 0 || (size_t)written >= sizeof(expected) ||
                has_field(response, "\"error\":") ||
                !has_field(response, "\"op\":\"find_lr_orders\"") ||
                !has_field(response, "\"best_order\"") ||
                !has_field(response, expected)) {
                fprintf(stderr,
                        "LibraryLink smoke: licensed strategy row %s failed\n",
                        strategies[index].name);
                ++failures;
            }
            free(response);
        }

        {
            static const struct success_case successes[] = {
                {"factor_table",
                 "factor_table",
                 hf_factor_table,
                 "{\"op\":\"factor_table\",\"xvars\":[\"x\"],"
                 "\"groups\":[[\"x\"]],\"order\":[\"x\"]}",
                 "\"polys\":[\"x\"]"},
                {"find_lr_orders_scan",
                 "find_lr_orders_scan",
                 hf_find_lr_orders_scan,
                 "{\"op\":\"find_lr_orders_scan\",\"xvars\":[\"x\"],"
                 "\"groups\":[[\"x\"]],\"exps\":[[[-1,0]]]}",
                 "\"orders\":"},
                {"hyperflint",
                 "hyperflint",
                 hf_hyperflint_sym,
                 "{\"op\":\"hyperflint\",\"vars\":[\"x\"],"
                 "\"vars_int\":[\"x\"],\"f\":\"1/(1+x)^2\","
                 "\"parallel\":false,\"check_divergences\":true}",
                 "\"result\":[{\"coef\":\"1\",\"key\":[]}]"},
            };
            for (index = 0; index < sizeof(successes) / sizeof(successes[0]); ++index) {
                char *response = NULL;
                char expected_op[96];
                const int written = snprintf(expected_op, sizeof(expected_op),
                                             "\"op\":\"%s\"", successes[index].op);
                if (!call_json(&callbacks, successes[index].function,
                               successes[index].request, &response) ||
                    written <= 0 || (size_t)written >= sizeof(expected_op) ||
                    has_field(response, "\"error\":") ||
                    !has_field(response, expected_op) ||
                    !has_field(response, successes[index].required_field)) {
                    fprintf(stderr,
                            "LibraryLink smoke: licensed success row %s failed\n",
                            successes[index].name);
                    ++failures;
                }
                free(response);
            }
        }
    }

    clear_result.integer = &clear_value;
    if (hf_clear_state(&callbacks, 0, NULL, clear_result) != LIBRARY_NO_ERROR ||
        clear_value != 0) {
        fputs("LibraryLink smoke: clear-state contract failed\n", stderr);
        ++failures;
    }

    if (version_copy != NULL) {
        char field[160];
        const int written = snprintf(field, sizeof(field),
                                     "\"hf_version\":\"%s\"", version_copy);
        char *response = NULL;
        if (written <= 0 || (size_t)written >= sizeof(field) ||
            !call_json(&callbacks, hf_find_lr_orders, "{bad}", &response) ||
            !has_field(response, field)) {
            fputs("LibraryLink smoke: direct/envelope version mismatch\n", stderr);
            ++failures;
        }
        free(response);
    }

    free(version_copy);
    WolframLibrary_uninitialize(&callbacks);
    if (failures == 0) {
        puts("LibraryLink smoke: lifecycle, UTF8String slots, delegation, state, and version PASS");
    }
    return failures == 0 ? 0 : 1;
}
