use std::collections::BTreeMap;

use crate::diagnostic::DiagnosticState;

pub trait RequestResponseSpy {
    fn apply_context_attrs(&self, attrs: &mut BTreeMap<String, String>);
    fn ensure_connection_context(&self) -> Option<String>;
    fn refresh_connection_context_if_dirty(&self);
    fn touch_connection_context(&self, entity_id: &str);
    fn emit_request_node(
        &self,
        request_node_id: String,
        method_name: String,
        attrs: BTreeMap<String, String>,
    );
    fn emit_response_node(
        &self,
        response_node_id: String,
        method_name: String,
        attrs: BTreeMap<String, String>,
    );
}

#[cfg(feature = "diagnostics")]
#[inline]
fn register_request_or_response_node(
    diag: &DiagnosticState,
    node_id: String,
    node_kind: peeps_types::NodeKind,
    method_name: String,
    mut attrs: BTreeMap<String, String>,
) {
    let _ = diag.ensure_connection_context();
    diag.refresh_connection_context_if_dirty();
    diag.apply_context_attrs(&mut attrs);
    let attrs_json = facet_json::to_string(&attrs).unwrap_or_else(|_| "{}".to_string());
    peeps::registry::register_node(peeps_types::Node {
        id: node_id.clone(),
        kind: node_kind,
        label: Some(method_name),
        attrs_json,
    });
    diag.touch_connection_context(&node_id);
}

#[cfg(feature = "diagnostics")]
impl RequestResponseSpy for DiagnosticState {
    #[inline]
    fn apply_context_attrs(&self, attrs: &mut BTreeMap<String, String>) {
        self.apply_connection_context_attrs(attrs);
    }

    #[inline]
    fn ensure_connection_context(&self) -> Option<String> {
        let rpc_connection = self.rpc_connection_token();
        if rpc_connection.is_empty() {
            return None;
        }
        Some(self.ensure_connection_context_id())
    }

    #[inline]
    fn refresh_connection_context_if_dirty(&self) {
        let _ = self.take_connection_context_refresh_if_dirty();
    }

    #[inline]
    fn touch_connection_context(&self, entity_id: &str) {
        if let Some(connection_context_id) = self.ensure_connection_context() {
            peeps::registry::touch_edge(entity_id, &connection_context_id);
        }
    }

    #[inline]
    fn emit_request_node(
        &self,
        request_node_id: String,
        method_name: String,
        attrs: BTreeMap<String, String>,
    ) {
        register_request_or_response_node(
            self,
            request_node_id,
            peeps_types::NodeKind::Request,
            method_name,
            attrs,
        );
    }

    #[inline]
    fn emit_response_node(
        &self,
        response_node_id: String,
        method_name: String,
        attrs: BTreeMap<String, String>,
    ) {
        register_request_or_response_node(
            self,
            response_node_id,
            peeps_types::NodeKind::Response,
            method_name,
            attrs,
        );
    }
}

#[cfg(not(feature = "diagnostics"))]
impl RequestResponseSpy for DiagnosticState {
    #[inline]
    fn apply_context_attrs(&self, _attrs: &mut BTreeMap<String, String>) {}

    #[inline]
    fn ensure_connection_context(&self) -> Option<String> {
        None
    }

    #[inline]
    fn refresh_connection_context_if_dirty(&self) {}

    #[inline]
    fn touch_connection_context(&self, _entity_id: &str) {}

    #[inline]
    fn emit_request_node(
        &self,
        _request_node_id: String,
        _method_name: String,
        _attrs: BTreeMap<String, String>,
    ) {
    }

    #[inline]
    fn emit_response_node(
        &self,
        _response_node_id: String,
        _method_name: String,
        _attrs: BTreeMap<String, String>,
    ) {
    }
}
