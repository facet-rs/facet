/**
 * @file styx.h
 * @brief C API for the Styx configuration language parser.
 *
 * This header provides a safe, modern C interface to the Styx parser.
 * It uses nullability annotations for better static analysis and includes
 * nodiscard attributes to prevent ignoring important return values.
 *
 * @example
 * ```c
 * StyxParseResult result = styx_parse("name Alice\nage 30");
 * if (result.document) {
 *     const StyxValue *name = styx_document_get(result.document, "name");
 *     if (name) {
 *         char *text = styx_value_scalar(name);
 *         if (text) {
 *             printf("name: %s\n", text);
 *             styx_free_string(text);
 *         }
 *     }
 *     styx_free_document(result.document);
 * } else {
 *     fprintf(stderr, "Error: %s\n", result.error);
 *     styx_free_string(result.error);
 * }
 * ```
 */

#ifndef STYX_H
#define STYX_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

/* Version information */
#define STYX_VERSION_MAJOR 0
#define STYX_VERSION_MINOR 1
#define STYX_VERSION_PATCH 0
#define STYX_VERSION_STRING "0.1.0"

/* ==========================================================================
 * Compiler feature detection and portability macros
 * ========================================================================== */

#ifdef __cplusplus
#  define STYX_EXTERN_C_BEGIN extern "C" {
#  define STYX_EXTERN_C_END }
#else
#  define STYX_EXTERN_C_BEGIN
#  define STYX_EXTERN_C_END
#endif

/* Nullability annotations (Clang only - these are not supported by GCC) */
#if defined(__clang__)
#  define STYX_NONNULL _Nonnull
#  define STYX_NULLABLE _Nullable
#else
#  define STYX_NONNULL
#  define STYX_NULLABLE
#endif

/* Nodiscard attribute (C++17, or compiler extension)
 * We use __attribute__ on GCC/Clang even in C23 mode because mixing GNU-style
 * attributes (STYX_API) with C23 [[nodiscard]] can cause issues in some cases.
 */
#if defined(__cplusplus) && __cplusplus >= 201703L
#  define STYX_NODISCARD [[nodiscard]]
#elif defined(__GNUC__) || defined(__clang__)
#  define STYX_NODISCARD __attribute__((warn_unused_result))
#elif defined(_MSC_VER)
#  define STYX_NODISCARD _Check_return_
#else
#  define STYX_NODISCARD
#endif

/* Export/import for shared libraries */
#if defined(_WIN32) || defined(__CYGWIN__)
#  ifdef STYX_BUILDING_DLL
#    define STYX_API __declspec(dllexport)
#  elif defined(STYX_USING_DLL)
#    define STYX_API __declspec(dllimport)
#  else
#    define STYX_API
#  endif
#elif defined(__GNUC__) && __GNUC__ >= 4
#  define STYX_API __attribute__((visibility("default")))
#else
#  define STYX_API
#endif

STYX_EXTERN_C_BEGIN

/* ==========================================================================
 * Type definitions
 * ========================================================================== */

/**
 * @brief Type of a Styx value's payload.
 */
typedef enum StyxPayloadKind {
    /** No payload (unit value or tag-only). */
    STYX_PAYLOAD_KIND_NONE = 0,
    /** Scalar text value. */
    STYX_PAYLOAD_KIND_SCALAR = 1,
    /** Sequence of values `(a b c)`. */
    STYX_PAYLOAD_KIND_SEQUENCE = 2,
    /** Object with key-value pairs `{k v}`. */
    STYX_PAYLOAD_KIND_OBJECT = 3
} StyxPayloadKind;

/** @brief Opaque handle to a parsed Styx document. */
typedef struct StyxDocument StyxDocument;

/** @brief Opaque handle to a Styx value. */
typedef struct StyxValue StyxValue;

/** @brief Opaque handle to a Styx object. */
typedef struct StyxObject StyxObject;

/** @brief Opaque handle to a Styx sequence. */
typedef struct StyxSequence StyxSequence;

/**
 * @brief Result of a parse operation.
 *
 * After parsing, exactly one of `document` or `error` will be non-null.
 * The caller must free whichever is non-null using the appropriate free function.
 */
typedef struct StyxParseResult {
    /**
     * @brief The parsed document, or NULL if parsing failed.
     * @note Must be freed with styx_free_document() when done.
     */
    StyxDocument *STYX_NULLABLE document;

    /**
     * @brief Error message if parsing failed, or NULL on success.
     * @note Must be freed with styx_free_string() when done.
     */
    char *STYX_NULLABLE error;
} StyxParseResult;

/* ==========================================================================
 * Parsing functions
 * ========================================================================== */

/**
 * @brief Parse a Styx document from a UTF-8 string.
 *
 * @param source A null-terminated UTF-8 string containing the Styx document.
 * @return A StyxParseResult. Check if `document` is non-null for success.
 *
 * @note The caller must free the result:
 *       - On success: call styx_free_document(result.document)
 *       - On failure: call styx_free_string(result.error)
 */
STYX_API STYX_NODISCARD
StyxParseResult styx_parse(const char *STYX_NONNULL source);

/**
 * @brief Free a parsed document.
 *
 * @param doc The document to free, or NULL (no-op if NULL).
 *
 * @warning After calling this function, the document pointer and any
 *          pointers obtained from it (values, objects, sequences) are invalid.
 */
STYX_API
void styx_free_document(StyxDocument *STYX_NULLABLE doc);

/**
 * @brief Free a string returned by the library.
 *
 * @param s The string to free, or NULL (no-op if NULL).
 */
STYX_API
void styx_free_string(char *STYX_NULLABLE s);

/* ==========================================================================
 * Document access
 * ========================================================================== */

/**
 * @brief Get the root object of a document.
 *
 * @param doc The document.
 * @return The root object, or NULL if doc is NULL.
 *
 * @note The returned pointer is valid as long as the document is not freed.
 */
STYX_API STYX_NODISCARD
const StyxObject *STYX_NULLABLE styx_document_root(
    const StyxDocument *STYX_NULLABLE doc);

/**
 * @brief Get a value by path from a document.
 *
 * Paths use `.` for object access and `[n]` for sequence indexing.
 * Example: "server.hosts[0].name"
 *
 * @param doc The document.
 * @param path A null-terminated path string.
 * @return The value at the path, or NULL if not found or invalid path.
 *
 * @note The returned pointer is valid as long as the document is not freed.
 */
STYX_API STYX_NODISCARD
const StyxValue *STYX_NULLABLE styx_document_get(
    const StyxDocument *STYX_NULLABLE doc,
    const char *STYX_NONNULL path);

/* ==========================================================================
 * Value inspection
 * ========================================================================== */

/**
 * @brief Get the payload kind of a value.
 *
 * @param value The value to inspect.
 * @return The payload kind, or STYX_PAYLOAD_KIND_NONE if value is NULL.
 */
STYX_API STYX_NODISCARD
StyxPayloadKind styx_value_payload_kind(const StyxValue *STYX_NULLABLE value);

/**
 * @brief Check if a value is unit (no tag and no payload).
 *
 * Unit values are written as `@` in Styx syntax.
 *
 * @param value The value to check.
 * @return true if the value is unit, false otherwise (or if value is NULL).
 */
STYX_API STYX_NODISCARD
bool styx_value_is_unit(const StyxValue *STYX_NULLABLE value);

/**
 * @brief Get the tag name of a value.
 *
 * Tags are prefixed with `@` in Styx syntax (e.g., `@string`, `@date`).
 *
 * @param value The value to inspect.
 * @return A newly allocated string with the tag name, or NULL if no tag.
 *
 * @note The returned string must be freed with styx_free_string().
 */
STYX_API STYX_NODISCARD
char *STYX_NULLABLE styx_value_tag(const StyxValue *STYX_NULLABLE value);

/**
 * @brief Get the scalar text content of a value.
 *
 * @param value The value to inspect.
 * @return A newly allocated string with the scalar text, or NULL if not a scalar.
 *
 * @note The returned string must be freed with styx_free_string().
 */
STYX_API STYX_NODISCARD
char *STYX_NULLABLE styx_value_scalar(const StyxValue *STYX_NULLABLE value);

/**
 * @brief Get the object payload of a value.
 *
 * @param value The value to inspect.
 * @return The object payload, or NULL if not an object.
 *
 * @note The returned pointer is valid as long as the parent document is not freed.
 */
STYX_API STYX_NODISCARD
const StyxObject *STYX_NULLABLE styx_value_as_object(
    const StyxValue *STYX_NULLABLE value);

/**
 * @brief Get the sequence payload of a value.
 *
 * @param value The value to inspect.
 * @return The sequence payload, or NULL if not a sequence.
 *
 * @note The returned pointer is valid as long as the parent document is not freed.
 */
STYX_API STYX_NODISCARD
const StyxSequence *STYX_NULLABLE styx_value_as_sequence(
    const StyxValue *STYX_NULLABLE value);

/**
 * @brief Get a nested value by path.
 *
 * @param value The starting value.
 * @param path A null-terminated path string (e.g., "foo.bar[0]").
 * @return The value at the path, or NULL if not found.
 *
 * @note The returned pointer is valid as long as the parent document is not freed.
 */
STYX_API STYX_NODISCARD
const StyxValue *STYX_NULLABLE styx_value_get(
    const StyxValue *STYX_NULLABLE value,
    const char *STYX_NONNULL path);

/* ==========================================================================
 * Object access
 * ========================================================================== */

/**
 * @brief Get the number of entries in an object.
 *
 * @param obj The object.
 * @return The number of entries, or 0 if obj is NULL.
 */
STYX_API STYX_NODISCARD
size_t styx_object_len(const StyxObject *STYX_NULLABLE obj);

/**
 * @brief Get a value from an object by key.
 *
 * @param obj The object.
 * @param key A null-terminated key string.
 * @return The value for the key, or NULL if not found.
 *
 * @note The returned pointer is valid as long as the parent document is not freed.
 */
STYX_API STYX_NODISCARD
const StyxValue *STYX_NULLABLE styx_object_get(
    const StyxObject *STYX_NULLABLE obj,
    const char *STYX_NONNULL key);

/**
 * @brief Get the key at a given index in an object.
 *
 * @param obj The object.
 * @param index The index (must be < styx_object_len(obj)).
 * @return The key value, or NULL if index is out of bounds.
 *
 * @note The returned pointer is valid as long as the parent document is not freed.
 */
STYX_API STYX_NODISCARD
const StyxValue *STYX_NULLABLE styx_object_key_at(
    const StyxObject *STYX_NULLABLE obj,
    size_t index);

/**
 * @brief Get the value at a given index in an object.
 *
 * @param obj The object.
 * @param index The index (must be < styx_object_len(obj)).
 * @return The value, or NULL if index is out of bounds.
 *
 * @note The returned pointer is valid as long as the parent document is not freed.
 */
STYX_API STYX_NODISCARD
const StyxValue *STYX_NULLABLE styx_object_value_at(
    const StyxObject *STYX_NULLABLE obj,
    size_t index);

/* ==========================================================================
 * Sequence access
 * ========================================================================== */

/**
 * @brief Get the number of items in a sequence.
 *
 * @param seq The sequence.
 * @return The number of items, or 0 if seq is NULL.
 */
STYX_API STYX_NODISCARD
size_t styx_sequence_len(const StyxSequence *STYX_NULLABLE seq);

/**
 * @brief Get an item from a sequence by index.
 *
 * @param seq The sequence.
 * @param index The index (must be < styx_sequence_len(seq)).
 * @return The item, or NULL if index is out of bounds.
 *
 * @note The returned pointer is valid as long as the parent document is not freed.
 */
STYX_API STYX_NODISCARD
const StyxValue *STYX_NULLABLE styx_sequence_get(
    const StyxSequence *STYX_NULLABLE seq,
    size_t index);

STYX_EXTERN_C_END

#endif /* STYX_H */
