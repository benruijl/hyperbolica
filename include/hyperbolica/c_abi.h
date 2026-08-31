#ifndef HYPERBOLICA_C_ABI_H
#define HYPERBOLICA_C_ABI_H

/*
 * Hyperbolica stable C ABI.
 *
 * Every operation accepts one readable, NUL-terminated UTF-8 JSON object and
 * returns a newly allocated, NUL-terminated UTF-8 JSON response.  The response
 * always contains `op`, `schema_version`, and `hf_version`; ordinary failures
 * are reported in-band through an `error` field.  A NULL return is reserved for
 * catastrophic allocation failure.
 *
 * Operation results are owned by the caller, but they MUST be released with
 * hf_free_string(), never with free() or a language-runtime allocator.
 * hf_free_string(NULL) is a no-op.  In contrast, hf_version_string() returns a
 * borrowed process-lifetime pointer which MUST NOT be freed.
 */

#define HF_SCHEMA_VERSION 2

#ifdef __cplusplus
extern "C" {
#endif

void hf_free_string(char *value);

char *hf_partial_fractions(const char *request_json);
char *hf_linear_factors(const char *request_json);
char *hf_find_lr_orders(const char *request_json);
char *hf_factor_table(const char *request_json);
char *hf_find_lr_orders_scan(const char *request_json);
char *hf_hyperflint_sym(const char *request_json);

/* MAJOR.MINOR.PATCH.BUILD, owned by the library for the process lifetime. */
const char *hf_version_string(void);

#ifdef __cplusplus
}
#endif

#endif
