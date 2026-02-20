use std::collections::BTreeMap;

use crate::diagnostic::DiagnosticState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseOutcome {
    Ok,
    Error,
    Cancelled,
}

#[derive(Clone, Default)]
pub struct TypedRequestHandle {
    #[cfg(feature = "diagnostics")]
    inner: Option<peeps::RpcRequestHandle>,
}

impl TypedRequestHandle {
    #[cfg(feature = "diagnostics")]
    fn from_inner(inner: peeps::RpcRequestHandle) -> Self {
        Self { inner: Some(inner) }
    }

    pub fn id_for_wire(&self) -> Option<String> {
        #[cfg(feature = "diagnostics")]
        {
            return self.inner.as_ref().map(|h| h.id_for_wire().to_string());
        }
        #[cfg(not(feature = "diagnostics"))]
        {
            None
        }
    }

    pub fn entity_handle(&self) -> Option<peeps::EntityHandle> {
        #[cfg(feature = "diagnostics")]
        {
            return self.inner.as_ref().map(|h| h.handle().clone());
        }
        #[cfg(not(feature = "diagnostics"))]
        {
            None
        }
    }
}

#[derive(Clone, Default)]
pub struct TypedResponseHandle {
    #[cfg(feature = "diagnostics")]
    inner: Option<peeps::EntityHandle<peeps_types::Response>>,
}

impl TypedResponseHandle {
    #[cfg(feature = "diagnostics")]
    fn from_inner(inner: peeps::EntityHandle<peeps_types::Response>) -> Self {
        Self { inner: Some(inner) }
    }

    pub fn entity_id_for_wire(&self) -> Option<String> {
        #[cfg(feature = "diagnostics")]
        {
            return self.inner.as_ref().map(|h| h.id().as_str().to_string());
        }
        #[cfg(not(feature = "diagnostics"))]
        {
            None
        }
    }

    pub fn mark(&self, outcome: ResponseOutcome) {
        #[cfg(not(feature = "diagnostics"))]
        let _ = outcome;
        #[cfg(feature = "diagnostics")]
        if let Some(handle) = &self.inner {
            let _ = handle.mutate(|body| {
                body.status = match outcome {
                    ResponseOutcome::Ok => {
                        peeps_types::ResponseStatus::Ok(peeps_types::Json::new("null"))
                    }
                    ResponseOutcome::Error => peeps_types::ResponseStatus::Error(
                        peeps_types::ResponseError::Internal(String::from("error")),
                    ),
                    ResponseOutcome::Cancelled => peeps_types::ResponseStatus::Cancelled,
                };
            });
        }
    }

    pub fn entity_handle(&self) -> Option<peeps::EntityHandle<peeps_types::Response>> {
        #[cfg(feature = "diagnostics")]
        {
            return self.inner.clone();
        }
        #[cfg(not(feature = "diagnostics"))]
        {
            None
        }
    }
}

pub trait RequestResponseSpy {
    fn apply_context_attrs(&self, attrs: &mut BTreeMap<String, String>);
    fn ensure_connection_context(&self) -> Option<String>;
    fn refresh_connection_context_if_dirty(&self);
    fn touch_connection_context(&self, entity_id: &str);
    fn emit_request_node(
        &self,
        full_method_name: String,
        body: peeps_types::RequestEntity,
        source: peeps::SourceId,
    ) -> TypedRequestHandle;
    fn emit_response_node(
        &self,
        full_method_name: String,
        body: peeps_types::ResponseEntity,
        source: peeps::SourceId,
        request_wire_id: Option<&str>,
    ) -> TypedResponseHandle;
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
    fn touch_connection_context(&self, _entity_id: &str) {
        let _ = self.ensure_connection_context();
        self.refresh_connection_context_if_dirty();
    }

    #[inline]
    fn emit_request_node(
        &self,
        full_method_name: String,
        body: peeps_types::RequestEntity,
        source: peeps::SourceId,
    ) -> TypedRequestHandle {
        let _ = self.ensure_connection_context();
        self.refresh_connection_context_if_dirty();
        let request = peeps::rpc_request_with_body(full_method_name, body, source);
        self.link_entity_to_connection_scope(request.handle());
        TypedRequestHandle::from_inner(request)
    }

    #[inline]
    fn emit_response_node(
        &self,
        full_method_name: String,
        body: peeps_types::ResponseEntity,
        source: peeps::SourceId,
        request_wire_id: Option<&str>,
    ) -> TypedResponseHandle {
        let _ = self.ensure_connection_context();
        self.refresh_connection_context_if_dirty();
        let response = if let Some(request_wire_id) = request_wire_id {
            let request_ref = peeps::entity_ref_from_wire(request_wire_id.to_owned());
            peeps::rpc_response_for_with_body(full_method_name, &request_ref, body, source)
        } else {
            peeps::rpc_response_with_body(full_method_name, body, source)
        };
        self.link_entity_to_connection_scope(&response);
        TypedResponseHandle::from_inner(response)
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
        _full_method_name: String,
        _body: peeps_types::RequestEntity,
        _source: peeps::SourceId,
    ) -> TypedRequestHandle {
        TypedRequestHandle::default()
    }

    #[inline]
    fn emit_response_node(
        &self,
        _full_method_name: String,
        _body: peeps_types::ResponseEntity,
        _source: peeps::SourceId,
        _request_wire_id: Option<&str>,
    ) -> TypedResponseHandle {
        TypedResponseHandle::default()
    }
}
