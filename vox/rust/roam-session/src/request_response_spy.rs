use std::collections::BTreeMap;
#[cfg(feature = "diagnostics")]
use std::sync::Arc;

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
    wire_id: Option<String>,
}

impl TypedRequestHandle {
    #[cfg(feature = "diagnostics")]
    fn from_wire_id(wire_id: String) -> Self {
        Self {
            wire_id: Some(wire_id),
        }
    }

    pub fn id_for_wire(&self) -> Option<String> {
        #[cfg(feature = "diagnostics")]
        {
            self.wire_id.clone()
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
    mark: Option<Arc<dyn Fn(ResponseOutcome) + Send + Sync + 'static>>,
}

impl TypedResponseHandle {
    #[cfg(feature = "diagnostics")]
    fn from_mark(mark: Arc<dyn Fn(ResponseOutcome) + Send + Sync + 'static>) -> Self {
        Self { mark: Some(mark) }
    }

    pub fn mark(&self, outcome: ResponseOutcome) {
        #[cfg(not(feature = "diagnostics"))]
        let _ = outcome;
        #[cfg(feature = "diagnostics")]
        if let Some(mark) = &self.mark {
            mark(outcome);
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
    ) -> TypedRequestHandle;
    fn emit_response_node(
        &self,
        full_method_name: String,
        body: moire_types::ResponseEntity,
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
    ) -> TypedRequestHandle {
        let _ = self.ensure_connection_context();
        self.refresh_connection_context_if_dirty();
        let request = moire::rpc::rpc_request_with_body(full_method_name, body);
        TypedRequestHandle::from_wire_id(request.id_for_wire())
    }

    #[inline]
    fn emit_response_node(
        &self,
        full_method_name: String,
        body: moire_types::ResponseEntity,
        request_wire_id: Option<&str>,
    ) -> TypedResponseHandle {
        let _ = request_wire_id;
        let _ = self.ensure_connection_context();
        self.refresh_connection_context_if_dirty();

        let response = moire::rpc::rpc_response_with_body(full_method_name, body);
        let mark = Arc::new(move |outcome: ResponseOutcome| {
            let _ = response.mutate(|body| {
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
        });
        TypedResponseHandle::from_mark(mark)
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
    ) -> TypedRequestHandle {
        TypedRequestHandle::default()
    }

    #[inline]
    fn emit_response_node(
        &self,
        _full_method_name: String,
        _body: moire_types::ResponseEntity,
        _request_wire_id: Option<&str>,
    ) -> TypedResponseHandle {
        TypedResponseHandle::default()
    }
}
