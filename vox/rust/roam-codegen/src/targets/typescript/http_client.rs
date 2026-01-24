//! TypeScript HTTP client generation.
//!
//! Generates a fetch()-based HTTP client for calling roam services via the HTTP bridge.
//! r[bridge.url.methods] - Methods are called via POST /{service}/{method}
//! r[bridge.request.body] - Arguments are sent as a JSON array
//! r[bridge.json.channels-forbidden] - Methods with channels throw at runtime
//! r[bridge.nonce.retry-safe] - Clients retrying requests should use the same nonce

use heck::{ToLowerCamelCase, ToUpperCamelCase};
use roam_schema::{ServiceDetail, is_rx, is_tx};

use super::types::ts_type;

/// Format a doc comment for TypeScript/JSDoc.
fn format_doc_comment(doc: &str, indent: &str) -> String {
    let lines: Vec<&str> = doc.lines().collect();

    if lines.is_empty() {
        return String::new();
    }

    if lines.len() == 1 {
        // Single line: /** doc */
        format!("{}/** {} */\n", indent, lines[0].trim())
    } else {
        // Multi-line: proper JSDoc format
        let mut out = format!("{}/**\n", indent);
        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                out.push_str(&format!("{} *\n", indent));
            } else {
                out.push_str(&format!("{} * {}\n", indent, trimmed));
            }
        }
        out.push_str(&format!("{} */\n", indent));
        out
    }
}

/// Generate HTTP client for a service.
///
/// The generated client uses fetch() to call methods via the HTTP bridge.
pub fn generate_http_client(service: &ServiceDetail) -> String {
    let mut out = String::new();
    let service_name = service.name.to_upper_camel_case();

    // Interface
    out.push_str(&format!(
        "// HTTP client for {service_name} (via roam HTTP bridge)\n"
    ));
    out.push_str(&format!("export interface {service_name}HttpCaller {{\n"));

    for method in &service.methods {
        let method_name = method.method_name.to_lower_camel_case();
        let has_channels = method.args.iter().any(|a| is_tx(a.ty) || is_rx(a.ty))
            || is_tx(method.return_type)
            || is_rx(method.return_type);

        // Build args (skip channel types for signature, they'll throw at runtime)
        let args: Vec<String> = method
            .args
            .iter()
            .filter(|a| !is_tx(a.ty) && !is_rx(a.ty))
            .map(|a| format!("{}: {}", a.name.to_lower_camel_case(), ts_type(a.ty)))
            .collect();

        let ret_ty = if has_channels {
            "never".to_string()
        } else {
            ts_type(method.return_type)
        };

        if let Some(doc) = &method.doc {
            out.push_str(&format_doc_comment(doc, "  "));
        }
        if has_channels {
            out.push_str("  /** @throws Channel methods require WebSocket */\n");
        }
        out.push_str(&format!(
            "  {method_name}({}): Promise<{ret_ty}>;\n",
            args.join(", ")
        ));
    }

    out.push_str("}\n\n");

    // Response type union
    out.push_str("// r[bridge.response.success], r[bridge.response.user-error], r[bridge.response.protocol-error]\n");
    out.push_str(&format!("export type {service_name}HttpResponse<T> =\n"));
    out.push_str("  | T  // success\n");
    out.push_str("  | { error: \"user\"; value: unknown }  // application error\n");
    out.push_str("  | { error: \"unknown_method\" }  // protocol error\n");
    out.push_str("  | { error: \"invalid_payload\" }  // protocol error\n");
    out.push_str("  | { error: \"cancelled\" };  // protocol error\n\n");

    // Error class
    out.push_str(&format!(
        "export class {service_name}HttpError extends Error {{\n"
    ));
    out.push_str("  constructor(\n");
    out.push_str("    public readonly kind: \"user\" | \"unknown_method\" | \"invalid_payload\" | \"cancelled\" | \"bridge\",\n");
    out.push_str("    public readonly value?: unknown,\n");
    out.push_str("    public readonly status?: number,\n");
    out.push_str("  ) {\n");
    out.push_str("    super(kind === \"user\" ? `User error: ${JSON.stringify(value)}` : kind);\n");
    out.push_str(&format!("    this.name = \"{service_name}HttpError\";\n"));
    out.push_str("  }\n");
    out.push_str("}\n\n");

    // Implementation
    out.push_str(&format!(
        "export class {service_name}HttpClient implements {service_name}HttpCaller {{\n"
    ));
    out.push_str("  constructor(\n");
    out.push_str("    private readonly baseUrl: string,\n");
    out.push_str("    private readonly options?: {\n");
    out.push_str("      headers?: Record<string, string>;\n");
    out.push_str("      fetch?: typeof fetch;\n");
    out.push_str("    },\n");
    out.push_str("  ) {}\n\n");

    // Helper method for making requests
    out.push_str("  private async request<T>(method: string, args: unknown[]): Promise<T> {\n");
    out.push_str("    const fetchFn = this.options?.fetch ?? fetch;\n");
    out.push_str("    // r[bridge.url.methods] - POST /{service}/{method}\n");
    out.push_str(&format!(
        "    const url = `${{this.baseUrl}}/{}/${{method}}`;\n",
        service.name
    ));
    out.push_str("    // r[bridge.request.content-type], r[bridge.request.body]\n");
    out.push_str("    const res = await fetchFn(url, {\n");
    out.push_str("      method: \"POST\",\n");
    out.push_str("      headers: {\n");
    out.push_str("        \"Content-Type\": \"application/json\",\n");
    out.push_str("        ...this.options?.headers,\n");
    out.push_str("      },\n");
    out.push_str("      body: JSON.stringify(args),\n");
    out.push_str("    });\n\n");
    out.push_str("    // r[bridge.response.bridge-error] - non-200 means bridge error\n");
    out.push_str("    if (!res.ok) {\n");
    out.push_str("      const body = await res.json().catch(() => ({}));\n");
    out.push_str(&format!(
        "      throw new {service_name}HttpError(\"bridge\", body.message, res.status);\n"
    ));
    out.push_str("    }\n\n");
    out.push_str("    const body = await res.json();\n\n");
    out.push_str("    // r[bridge.response.protocol-error], r[bridge.response.user-error]\n");
    out.push_str("    if (body && typeof body === \"object\" && \"error\" in body) {\n");
    out.push_str("      const err = body as { error: string; value?: unknown };\n");
    out.push_str("      if (err.error === \"user\") {\n");
    out.push_str(&format!(
        "        throw new {service_name}HttpError(\"user\", err.value);\n"
    ));
    out.push_str("      }\n");
    out.push_str(&format!(
        "      throw new {service_name}HttpError(err.error as \"unknown_method\" | \"invalid_payload\" | \"cancelled\");\n"
    ));
    out.push_str("    }\n\n");
    out.push_str("    // r[bridge.response.success]\n");
    out.push_str("    return body as T;\n");
    out.push_str("  }\n\n");

    // Generate method implementations
    for method in &service.methods {
        let method_name = method.method_name.to_lower_camel_case();
        let has_channels = method.args.iter().any(|a| is_tx(a.ty) || is_rx(a.ty))
            || is_tx(method.return_type)
            || is_rx(method.return_type);

        // Build args (skip channel types)
        let args: Vec<String> = method
            .args
            .iter()
            .filter(|a| !is_tx(a.ty) && !is_rx(a.ty))
            .map(|a| format!("{}: {}", a.name.to_lower_camel_case(), ts_type(a.ty)))
            .collect();

        let arg_names: Vec<String> = method
            .args
            .iter()
            .filter(|a| !is_tx(a.ty) && !is_rx(a.ty))
            .map(|a| a.name.to_lower_camel_case())
            .collect();

        let ret_ty = if has_channels {
            "never".to_string()
        } else {
            ts_type(method.return_type)
        };

        out.push_str(&format!(
            "  async {method_name}({}): Promise<{ret_ty}> {{\n",
            args.join(", ")
        ));

        if has_channels {
            // r[bridge.json.channels-forbidden]
            out.push_str("    // r[bridge.json.channels-forbidden]\n");
            out.push_str(&format!(
                "    throw new {service_name}HttpError(\"bridge\", \"Channel methods require WebSocket\");\n"
            ));
        } else {
            out.push_str(&format!(
                "    return this.request<{ret_ty}>(\"{}\", [{}]);\n",
                method.method_name,
                arg_names.join(", ")
            ));
        }

        out.push_str("  }\n\n");
    }

    out.push_str("}\n");

    out
}

// Tests for HTTP client generation are done via integration tests
// using real service definitions from the xtask codegen command.
