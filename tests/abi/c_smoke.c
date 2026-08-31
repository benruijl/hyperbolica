#include "hyperbolica/c_abi.h"

#include <stddef.h>
#include <stdio.h>
#include <string.h>

#if defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L
_Static_assert(HF_SCHEMA_VERSION == 2, "unexpected Hyperbolica schema version");
#endif

typedef char *(*hf_operation)(const char *request_json);

struct operation_case {
    const char *op;
    hf_operation operation;
};

static int check_error_envelope(const struct operation_case *test_case) {
    char *response = test_case->operation(NULL);
    int ok = response != NULL;

    if (ok) {
        char op_field[128];
        const int written = snprintf(op_field, sizeof(op_field),
                                     "\"op\":\"%s\"", test_case->op);
        ok = written > 0 && (size_t)written < sizeof(op_field) &&
             strstr(response, op_field) != NULL &&
             strstr(response, "\"schema_version\":2") != NULL &&
             strstr(response, "\"hf_version\":") != NULL &&
             strstr(response, "\"error\":") != NULL &&
             strstr(response, "NULL") != NULL;

        /* The operation result is caller-owned writable storage. */
        if (response[0] != '\0') {
            const char first = response[0];
            response[0] = '#';
            response[0] = first;
        }
    }

    hf_free_string(response);
    return ok;
}

static int valid_four_component_version(const char *version) {
    unsigned int major = 0;
    unsigned int minor = 0;
    unsigned int patch = 0;
    unsigned int build = 0;
    char trailing = '\0';

    return version != NULL &&
           sscanf(version, "%u.%u.%u.%u%c", &major, &minor, &patch, &build,
                  &trailing) == 4;
}

int main(void) {
    static const struct operation_case operations[] = {
        {"partial_fractions", hf_partial_fractions},
        {"linear_factors", hf_linear_factors},
        {"find_lr_orders", hf_find_lr_orders},
        {"factor_table", hf_factor_table},
        {"find_lr_orders_scan", hf_find_lr_orders_scan},
        {"hyperflint", hf_hyperflint_sym},
    };
    size_t index = 0;
    int failures = 0;
    const char *version = hf_version_string();

    if (!valid_four_component_version(version) || version != hf_version_string()) {
        fputs("C ABI smoke: invalid or unstable version pointer\n", stderr);
        ++failures;
    }

    for (index = 0; index < sizeof(operations) / sizeof(operations[0]); ++index) {
        if (!check_error_envelope(&operations[index])) {
            fprintf(stderr, "C ABI smoke: malformed NULL-input response for %s\n",
                    operations[index].op);
            ++failures;
        }
    }

    hf_free_string(NULL);
    if (failures == 0) {
        puts("C ABI smoke: ownership, errors, schema, and version PASS");
    }
    return failures == 0 ? 0 : 1;
}
