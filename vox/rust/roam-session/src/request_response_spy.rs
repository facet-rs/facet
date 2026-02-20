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
    inner: Option<moire::RpcRequestHandle>,
}

impl TypedRequestHandle {
    #[cfg(feature = "diagnostics")]
    fn from_inner(inner: moire::RpcRequestHandle) -> Self {
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

    pub fn entity_handle(&self) -> Option<moire::EntityHandle> {
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
    inner: Option<moire::EntityHandle<moire_types::Response>>,
}

impl TypedResponseHandle {
    #[cfg(feature = "diagnostics")]
    fn from_inner(inner: moire::EntityHandle<moire_types::Response>) -> Self {
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
                        moire_types::ResponseStatus::Ok(moire_types::Json::new("null"))
                    }
                    ResponseOutcome::Error => moire_types::ResponseStatus::Error(
                        moire_types::ResponseError::Internal(String::from("error")),
                    ),
                    ResponseOutcome::Cancelled => moire_types::ResponseStatus::Cancelled,
                };
            });
        }
    }

    pub fn entity_handle(&self) -> Option<moire::EntityHandle<moire_types::Response>> {
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
        body: moire_types::RequestEntity,
        source: moire::SourceId,
    ) -> TypedRequestHandle;
    fn emit_response_node(
        &self,
        full_method_name: String,
        body: moire_types::ResponseEntity,
        source: moire::SourceId,
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
        body: moire_types::RequestEntity,
        source: moire::SourceId,
    ) -> TypedRequestHandle {
        let _ = self.ensure_connection_context();
        self.refresh_connection_context_if_dirty();
        let request = moire::rpc_request_with_body(full_method_name, body, source);
        self.link_entity_to_connection_scope(request.handle());
        TypedRequestHandle::from_inner(request)
    }

    #[inline]
    fn emit_response_node(
        &self,
        full_method_name: String,
        body: moire_types::ResponseEntity,
        source: moire::SourceId,
        request_wire_id: Option<&str>,
    ) -> TypedResponseHandle {
        let _ = self.ensure_connection_context();
        self.refresh_connection_context_if_dirty();
        let response = if let Some(request_wire_id) = request_wire_id {
            let request_ref = moire::entity_ref_from_wire(request_wire_id.to_owned());
            moire::rpc_response_for_with_body(full_method_name, &request_ref, body, source)
        } else {
            moire::rpc_response_with_body(full_method_name, body, source)
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
        _body: moire_types::RequestEntity,
        _source: moire::SourceId,
    ) -> TypedRequestHandle {
        TypedRequestHandle::default()
    }

    #[inline]
    fn emit_response_node(
        &self,
        _full_method_name: String,
        _body: moire_types::ResponseEntity,
        _source: moire::SourceId,
        _request_wire_id: Option<&str>,
    ) -> TypedResponseHandle {
        TypedResponseHandle::default()
    }
}
