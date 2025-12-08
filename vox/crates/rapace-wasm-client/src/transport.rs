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
    pub async fn send(&self, data: &[u8]) -> Result<(), JsValue> {
        if *self.closed.borrow() {
            return Err(JsValue::from_str("WebSocket is closed"));
        }

        if let Some(err) = self.error.borrow().as_ref() {
            return Err(JsValue::from_str(err));
        }

        self.ws.send_with_u8_array(data)
    }

    /// Receive binary data.
    ///
    /// This polls until a message is available.
    pub async fn recv(&mut self) -> Result<Vec<u8>, JsValue> {
        loop {
            // Check for errors
            if let Some(err) = self.error.borrow().as_ref() {
                return Err(JsValue::from_str(err));
            }

            // Check if closed
            if *self.closed.borrow() && self.received.borrow().is_empty() {
                return Err(JsValue::from_str("WebSocket closed"));
            }

            // Check for received data
            if let Some(data) = self.received.borrow_mut().pop_front() {
                return Ok(data);
            }

            // Yield to allow other tasks to run and messages to arrive
            gloo_timers::future::TimeoutFuture::new(1).await;
        }
    }

    /// Close the WebSocket.
    pub fn close(&self) {
        let _ = self.ws.close();
    }

    /// Check if the WebSocket is closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.borrow()
    }
}
