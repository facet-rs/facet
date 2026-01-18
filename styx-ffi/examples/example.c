/**
 * Example usage of the Styx C API.
 *
 * Compile with (from the examples directory):
 *   cc -o example example.c -I../include -L../../../target/release -lstyx_ffi
 *
 * Run with:
 *   # macOS
 *   DYLD_LIBRARY_PATH=../../../target/release ./example
 *   # Linux
 *   LD_LIBRARY_PATH=../../../target/release ./example
 */

#include <stdio.h>
#include <stdlib.h>
#include <styx.h>

int main(void) {
    const char *source =
        "name Alice\n"
        "age 30\n"
        "tags (developer rust python)\n"
        "address {\n"
        "  city \"New York\"\n"
        "  zip 10001\n"
        "}\n";

    printf("Parsing Styx document:\n%s\n", source);

    // Parse the document
    struct StyxParseResult result = styx_parse(source);

    if (result.error) {
        fprintf(stderr, "Parse error: %s\n", result.error);
        styx_free_string(result.error);
        return 1;
    }

    printf("Parse successful!\n\n");

    // Get values by path
    const struct StyxValue *name = styx_document_get(result.document, "name");
    if (name) {
        char *text = styx_value_scalar(name);
        if (text) {
            printf("name: %s\n", text);
            styx_free_string(text);
        }
    }

    const struct StyxValue *age = styx_document_get(result.document, "age");
    if (age) {
        char *text = styx_value_scalar(age);
        if (text) {
            printf("age: %s\n", text);
            styx_free_string(text);
        }
    }

    // Access nested value
    const struct StyxValue *city = styx_document_get(result.document, "address.city");
    if (city) {
        char *text = styx_value_scalar(city);
        if (text) {
            printf("address.city: %s\n", text);
            styx_free_string(text);
        }
    }

    // Access sequence
    const struct StyxValue *tags = styx_document_get(result.document, "tags");
    if (tags) {
        const struct StyxSequence *seq = styx_value_as_sequence(tags);
        if (seq) {
            uintptr_t len = styx_sequence_len(seq);
            printf("tags (%zu items):", (size_t)len);
            for (uintptr_t i = 0; i < len; i++) {
                const struct StyxValue *item = styx_sequence_get(seq, i);
                if (item) {
                    char *text = styx_value_scalar(item);
                    if (text) {
                        printf(" %s", text);
                        styx_free_string(text);
                    }
                }
            }
            printf("\n");
        }
    }

    // Iterate over root object
    printf("\nIterating over root object:\n");
    const struct StyxObject *root = styx_document_root(result.document);
    uintptr_t len = styx_object_len(root);
    for (uintptr_t i = 0; i < len; i++) {
        const struct StyxValue *key = styx_object_key_at(root, i);
        const struct StyxValue *value = styx_object_value_at(root, i);

        char *key_text = styx_value_scalar(key);
        enum StyxPayloadKind kind = styx_value_payload_kind(value);

        const char *kind_str;
        switch (kind) {
            case STYX_PAYLOAD_KIND_NONE: kind_str = "none"; break;
            case STYX_PAYLOAD_KIND_SCALAR: kind_str = "scalar"; break;
            case STYX_PAYLOAD_KIND_SEQUENCE: kind_str = "sequence"; break;
            case STYX_PAYLOAD_KIND_OBJECT: kind_str = "object"; break;
            default: kind_str = "unknown"; break;
        }

        printf("  %s: %s\n", key_text ? key_text : "(null)", kind_str);

        if (key_text) styx_free_string(key_text);
    }

    // Clean up
    styx_free_document(result.document);

    printf("\nDone!\n");
    return 0;
}
