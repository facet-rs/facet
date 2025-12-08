//! WebSocket transport for wasm using web_sys::WebSocket.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{BinaryType, CloseEvent, ErrorEvent, MessageEvent, WebSocket};

/// A wasm-compatible WebSocket wrapper.
pub struct WasmWebSocket {
    ws: WebSocket,
    /// Received messages queue.
    received: Rc<RefCell<VecDeque<Vec<u8>>>>,
    /// Error that occurred.
    error: Rc<RefCell<Option<String>>>,
    /// Whether the connection is closed.
    closed: Rc<RefCell<bool>>,
}

impl WasmWebSocket {
    /// Connect to a WebSocket server.
    pub async fn connect(url: &str) -> Result<Self, JsValue> {
        let ws = WebSocket::new(url)?;
        ws.set_binary_type(BinaryType::Arraybuffer);

        let received = Rc::new(RefCell::new(VecDeque::new()));
        let error: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let closed = Rc::new(RefCell::new(false));

        // Wait for connection to open first (before setting up persistent handlers)
        let open_result: Rc<RefCell<Option<Result<(), String>>>> = Rc::new(RefCell::new(None));

        {
            let open_result_clone = Rc::clone(&open_result);
            let onopen = Closure::<dyn FnMut()>::once(move || {
                *open_result_clone.borrow_mut() = Some(Ok(()));
            });
            ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
            onopen.forget();
        }

        {
            let open_result_clone = Rc::clone(&open_result);
            let onerror = Closure::<dyn FnMut(ErrorEvent)>::once(move |e: ErrorEvent| {
                let msg = e.message();
                let err_msg = if msg.is_empty() {
                    "WebSocket connection failed".to_string()
                } else {
                    msg
                };
                *open_result_clone.borrow_mut() = Some(Err(err_msg));
            });
            ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
            onerror.forget();
        }

        // Poll until connection opens or errors
        loop {
            if let Some(result) = open_result.borrow_mut().take() {
                match result {
                    Ok(()) => break,
                    Err(msg) => return Err(JsValue::from_str(&msg)),
                }
            }
            gloo_timers::future::TimeoutFuture::new(10).await;
        }

        // Now set up persistent handlers for the open connection

        // Set up message handler
        {
            let received = Rc::clone(&received);
            let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
                if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
                    let array = js_sys::Uint8Array::new(&abuf);
                    let data = array.to_vec();
                    received.borrow_mut().push_back(data);
                }
            });
            ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            onmessage.forget();
        }

        // Set up error handler
        {
            let error = Rc::clone(&error);
            let onerror = Closure::<dyn FnMut(ErrorEvent)>::new(move |e: ErrorEvent| {
                *error.borrow_mut() = Some(format!("WebSocket error: {:?}", e.message()));
            });
            ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
            onerror.forget();
        }

        // Set up close handler
        {
            let closed = Rc::clone(&closed);
            let onclose = Closure::<dyn FnMut(CloseEvent)>::new(move |_: CloseEvent| {
                *closed.borrow_mut() = true;
            });
            ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
            onclose.forget();
        }

        Ok(Self {
            ws,
            received,
            error,
            closed,
        })
    }

    /// Send binary data.
    ///
    /// This is synchronous since WebSocket.send() doesn't block.
    pub fn send(&self, data: &[u8]) -> Result<(), JsValue> {
        if *self.closed.borrow() {
            return Err(JsValue::from_str("WebSocket is closed"));
        }

        if let Some(err) = self.error.borrow().as_ref() {
            return Err(JsValue::from_str(err));
        }

        self.ws.send_with_u8_array(data)
    }

    /// Try to receive binary data without blocking.
    ///
    /// Returns `Ok(Some(data))` if data is available, `Ok(None)` if no data yet,
    /// or `Err` if the socket is closed or errored.
    pub fn try_recv(&self) -> Result<Option<Vec<u8>>, JsValue> {
        // Check for errors
        if let Some(err) = self.error.borrow().as_ref() {
            return Err(JsValue::from_str(err));
        }

        // Check for received data
        if let Some(data) = self.received.borrow_mut().pop_front() {
            return Ok(Some(data));
        }

        // Check if closed (after checking for data, since there might be buffered messages)
        if *self.closed.borrow() {
            return Err(JsValue::from_str("WebSocket closed"));
        }

        Ok(None)
    }

    /// Close the WebSocket.
    pub fn close(&self) {
        let _ = self.ws.close();
    }

    /// Check if the WebSocket is closed.
    #[allow(dead_code)]
    pub fn is_closed(&self) -> bool {
        *self.closed.borrow()
    }
}

/// Receive binary data from a WebSocket wrapped in Rc<RefCell<...>>.
///
/// This is a free function to avoid holding the RefCell borrow across await points.
pub async fn recv_from(ws: &Rc<RefCell<WasmWebSocket>>) -> Result<Vec<u8>, JsValue> {
    loop {
        // Try to get data without holding borrow across await.
        // We use a block to ensure the borrow is dropped before the await.
        let result = {
            let ws_ref = ws.borrow();
            ws_ref.try_recv()
        };

        match result? {
            Some(data) => return Ok(data),
            None => {
                // Yield to allow other tasks to run and messages to arrive
                gloo_timers::future::TimeoutFuture::new(1).await;
            }
        }
    }
}
