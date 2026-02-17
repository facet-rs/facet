use std::collections::BTreeMap;

use crate::diagnostic::DiagnosticState;

pub trait RequestResponseSpy {
    fn apply_connection_attrs(&self, attrs: &mut BTreeMap<String, String>);
    fn register_connection_context(&self) -> Option<String>;
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
    diag.apply_connection_attrs(&mut attrs);
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
    fn apply_connection_attrs(&self, attrs: &mut BTreeMap<String, String>) {
        let connection = self.connection_identity();
        attrs.insert("rpc.connection".to_string(), self.rpc_connection_token());
        attrs.insert("connection.src".to_string(), connection.src);
        attrs.insert("connection.dst".to_string(), connection.dst);
        attrs.insert("connection.transport".to_string(), connection.transport);
    }

    #[inline]
    fn register_connection_context(&self) -> Option<String> {
        let rpc_connection = self.rpc_connection_token();
        if rpc_connection.is_empty() {
            return None;
        }

        let mut attrs = BTreeMap::new();
        self.apply_connection_attrs(&mut attrs);
        attrs.insert(
            "connection.state".to_string(),
            self.connection_state().to_string(),
        );
        attrs.insert(
            "connection.opened_at_ns".to_string(),
            self.connection_identity().opened_at_ns.to_string(),
        );
        if let Some(closed_at_ns) = self.connection_closed_at_ns() {
            attrs.insert(
                "connection.closed_at_ns".to_string(),
                closed_at_ns.to_string(),
            );
        }
        if let Some(last_sent_at_ns) = self.last_frame_sent_at_ns() {
            attrs.insert(
                "connection.last_frame_sent_at_ns".to_string(),
                last_sent_at_ns.to_string(),
            );
        }
        if let Some(last_recv_at_ns) = self.last_frame_received_at_ns() {
            attrs.insert(
                "connection.last_frame_recv_at_ns".to_string(),
                last_recv_at_ns.to_string(),
            );
        }
        attrs.insert(
            "connection.pending_requests".to_string(),
            self.pending_requests().to_string(),
        );
        attrs.insert(
            "connection.pending_requests_outgoing".to_string(),
            self.pending_requests_outgoing().to_string(),
        );
        attrs.insert(
            "connection.pending_responses".to_string(),
            self.pending_responses().to_string(),
        );
        attrs.insert(
            "connection.driver.last_arm".to_string(),
            self.last_driver_arm(),
        );
        attrs.insert(
            "connection.driver.last_arm_at_ns".to_string(),
            self.last_driver_arm_at_ns().to_string(),
        );
        attrs.insert(
            "connection.driver.driver_rx_hits".to_string(),
            self.driver_arm_driver_rx_hits().to_string(),
        );
        attrs.insert(
            "connection.driver.io_recv_hits".to_string(),
            self.driver_arm_io_recv_hits().to_string(),
        );
        attrs.insert(
            "connection.driver.incoming_response_hits".to_string(),
            self.driver_arm_incoming_response_hits().to_string(),
        );
        attrs.insert(
            "connection.driver.sweep_hits".to_string(),
            self.driver_arm_sweep_hits().to_string(),
        );
        attrs.insert(
            "connection.pending_map.inserts".to_string(),
            self.pending_map_inserts().to_string(),
        );
        attrs.insert(
            "connection.pending_map.removes".to_string(),
            self.pending_map_removes().to_string(),
        );
        attrs.insert(
            "connection.pending_map.failures".to_string(),
            self.pending_map_failures().to_string(),
        );
        attrs.insert(
            "connection.pending_map.last_event".to_string(),
            self.pending_map_last_event(),
        );
        attrs.insert(
            "connection.pending_map.last_conn_id".to_string(),
            self.pending_map_last_conn_id().to_string(),
        );
        attrs.insert(
            "connection.pending_map.last_request_id".to_string(),
            self.pending_map_last_request_id().to_string(),
        );
        attrs.insert(
            "connection.pending_map.last_len_before".to_string(),
            self.pending_map_last_len_before().to_string(),
        );
        attrs.insert(
            "connection.pending_map.last_len_after".to_string(),
            self.pending_map_last_len_after().to_string(),
        );
        attrs.insert(
            "connection.pending_map.last_at_ns".to_string(),
            self.pending_map_last_at_ns().to_string(),
        );

        let connection_node_id = format!("connection:{rpc_connection}");
        let attrs_json = facet_json::to_string(&attrs).unwrap_or_else(|_| "{}".to_string());
        peeps::registry::register_node(peeps_types::Node {
            id: connection_node_id.clone(),
            kind: peeps_types::NodeKind::Connection,
            label: Some(rpc_connection),
            attrs_json,
        });
        Some(connection_node_id)
    }

    #[inline]
    fn touch_connection_context(&self, entity_id: &str) {
        if let Some(connection_node_id) = self.register_connection_context() {
            peeps::registry::touch_edge(entity_id, &connection_node_id);
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
    fn apply_connection_attrs(&self, _attrs: &mut BTreeMap<String, String>) {}

    #[inline]
    fn register_connection_context(&self) -> Option<String> {
        None
    }

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
