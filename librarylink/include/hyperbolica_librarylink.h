#ifndef HYPERBOLICA_LIBRARYLINK_H
#define HYPERBOLICA_LIBRARYLINK_H

/*
 * Header for Hyperbolica's Wolfram LibraryLink adapter.
 *
 * Define HYPERBOLICA_USE_SYSTEM_WOLFRAM_LIBRARY and put the Wolfram SDK's
 * IncludeFiles/C directory on the include path to compile against the vendor
 * header.  The fallback below is an ABI-compatible subset for build and smoke
 * tests on machines without a Wolfram installation; it is not a replacement
 * SDK for implementing arbitrary LibraryLink callbacks.
 */

#if defined(HYPERBOLICA_USE_SYSTEM_WOLFRAM_LIBRARY)
#include <WolframLibrary.h>
#else

#include <stddef.h>
#include <stdint.h>

typedef intptr_t mint;
typedef int mbool;
typedef double mreal;

typedef union st_MData {
    mbool *boolean;
    mint *integer;
    mreal *real;
    void **cmplex;
    void **tensor;
    void **sparse;
    void **numeric;
    void **image;
    char **utf8string;
} MArgument;

/* The adapter accesses only this stable first-field prefix. */
typedef struct st_WolframLibraryData {
    void (*UTF8String_disown)(char *value);
} *WolframLibraryData;

#define WolframLibraryVersion 6
#define LIBRARY_NO_ERROR 0
#define LIBRARY_FUNCTION_ERROR 6

#endif

#ifdef __cplusplus
extern "C" {
#endif

mint WolframLibrary_getVersion(void);
int WolframLibrary_initialize(WolframLibraryData lib_data);
void WolframLibrary_uninitialize(WolframLibraryData lib_data);

int hf_version(WolframLibraryData lib_data, mint argc,
               MArgument *args, MArgument result);
int hf_clear_state(WolframLibraryData lib_data, mint argc,
                   MArgument *args, MArgument result);
int hf_find_lr_orders(WolframLibraryData lib_data, mint argc,
                      MArgument *args, MArgument result);
int hf_factor_table(WolframLibraryData lib_data, mint argc,
                    MArgument *args, MArgument result);
int hf_find_lr_orders_scan(WolframLibraryData lib_data, mint argc,
                           MArgument *args, MArgument result);
int hf_hyperflint_sym(WolframLibraryData lib_data, mint argc,
                      MArgument *args, MArgument result);

#ifdef __cplusplus
}
#endif

#endif

