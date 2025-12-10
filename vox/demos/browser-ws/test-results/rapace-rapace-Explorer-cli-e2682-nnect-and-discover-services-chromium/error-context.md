# Page snapshot

```yaml
- generic [active] [ref=e1]:
  - heading "rapace Explorer Demo" [level=1] [ref=e2]
  - generic [ref=e3]:
    - heading "Connection Connected" [level=2] [ref=e4]:
      - text: Connection
      - generic [ref=e5]: Connected
    - textbox "WebSocket URL" [disabled] [ref=e6]: ws://127.0.0.1:4268
    - button "Connect" [disabled] [ref=e7]
    - button "Disconnect" [ref=e8] [cursor=pointer]
  - generic [ref=e9]:
    - heading "Services" [level=2] [ref=e10]
    - generic [ref=e11]:
      - generic [ref=e12] [cursor=pointer]:
        - strong [ref=e13]: Calculator
        - text: 3 methods
      - generic [ref=e14] [cursor=pointer]:
        - strong [ref=e15]: Greeter
        - text: 2 methods
      - generic [ref=e16] [cursor=pointer]:
        - strong [ref=e17]: Counter
        - text: 2 methods
    - button "Refresh Services" [ref=e18] [cursor=pointer]
  - generic [ref=e19]:
    - heading "Log" [level=2] [ref=e20]
    - button "Clear" [ref=e21] [cursor=pointer]
    - generic [ref=e22]: "[11:19:19 PM] Page loaded. Enter WebSocket URL and click \"Connect\" to start. [11:19:19 PM] Initializing WASM module... [11:19:19 PM] Connecting to ws://127.0.0.1:4268... [11:19:19 PM] Connected! [11:19:19 PM] Discovering services... [11:19:19 PM] Found 3 service(s)"
```