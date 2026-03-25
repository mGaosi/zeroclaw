# Quickstart: Host-Side Tool Registration

**Feature**: 002-host-tool-registration

## Rust-Native Usage

```rust
use zeroclaw::api::{
    lifecycle::init,
    host_tools::{register_tool, unregister_tool, setup_tool_handler, submit_tool_response},
    types::{HostToolSpec, ToolRequest, ToolResponse},
};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Initialize the agent
    let handle = init(None, None).await?;

    // 2. Set up the tool request handler
    let (tx, mut rx) = mpsc::unbounded_channel::<ToolRequest>();
    setup_tool_handler(&handle, tx)?;

    // 3. Spawn a task to handle tool requests
    tokio::spawn(async move {
        while let Some(request) = rx.recv().await {
            println!("Tool request: {} with args: {}", request.tool_name, request.arguments);

            // Execute the tool logic (your implementation)
            let output = match request.tool_name.as_str() {
                "get_device_battery" => "85%".to_string(),
                "get_gps_location" => r#"{"lat": 37.7749, "lng": -122.4194}"#.to_string(),
                _ => "Unknown tool".to_string(),
            };

            // 4. Submit the response
            let response = ToolResponse {
                request_id: request.request_id,
                output,
                success: true,
            };
            let _ = submit_tool_response(&handle, response);
        }
    });

    // 5. Register host tools
    let battery_tool_id = register_tool(&handle, HostToolSpec {
        name: "get_device_battery".into(),
        description: "Returns the current device battery percentage".into(),
        parameters_schema: r#"{"type": "object", "properties": {}}"#.into(),
        timeout_seconds: None, // uses default 30s
    })?;

    let gps_tool_id = register_tool(&handle, HostToolSpec {
        name: "get_gps_location".into(),
        description: "Returns the device's current GPS coordinates as {lat, lng}".into(),
        parameters_schema: r#"{"type": "object", "properties": {"accuracy": {"type": "string", "enum": ["high", "low"]}}}"#.into(),
        timeout_seconds: Some(10),
    })?;

    // 6. Send a message — the agent can now invoke these tools
    let (msg_tx, mut msg_rx) = mpsc::channel(64);
    zeroclaw::api::conversation::send_message(
        &handle,
        "What's my battery level and where am I?".into(),
        msg_tx,
    )?;

    while let Some(event) = msg_rx.recv().await {
        match event {
            zeroclaw::api::types::StreamEvent::Chunk { delta } => print!("{delta}"),
            zeroclaw::api::types::StreamEvent::ToolCall { tool, arguments } => {
                println!("\n[Tool Call] {tool}({arguments})");
            }
            zeroclaw::api::types::StreamEvent::ToolResult { tool, output, success } => {
                println!("[Tool Result] {tool}: {output} (success={success})");
            }
            zeroclaw::api::types::StreamEvent::Done { .. } => break,
            zeroclaw::api::types::StreamEvent::Error { message } => {
                eprintln!("Error: {message}");
                break;
            }
        }
    }

    // 7. Dynamic unregistration
    unregister_tool(&handle, gps_tool_id)?;
    // GPS tool is no longer available to the agent

    Ok(())
}
```

## Flutter/Dart Usage (via FRB)

```dart
import 'package:zeroclaw/zeroclaw.dart';

class ZeroClawService {
  AgentHandle? _handle;

  Future<void> initialize() async {
    // 1. Initialize the agent
    _handle = await init();

    // 2. Set up the tool request stream
    setupToolHandlerStream(_handle!, onToolRequest: (ToolRequest request) {
      _handleToolRequest(request);
    });

    // 3. Register tools
    final batteryToolId = registerTool(_handle!, HostToolSpec(
      name: 'get_device_battery',
      description: 'Returns the current device battery percentage',
      parametersSchema: '{"type": "object", "properties": {}}',
    ));
  }

  void _handleToolRequest(ToolRequest request) async {
    String output;
    bool success = true;

    switch (request.toolName) {
      case 'get_device_battery':
        final battery = await Battery().batteryLevel;
        output = '$battery%';
        break;
      default:
        output = 'Unknown tool';
        success = false;
    }

    // Return the result to ZeroClaw
    submitToolResponse(_handle!, ToolResponse(
      requestId: request.requestId,
      output: output,
      success: success,
    ));
  }
}
```

## Key Points

1. **Order matters**: Call `setup_tool_handler` before registering tools, so the channel is ready when invocations arrive.
2. **Registration requires init**: `register_tool` fails with `ApiError::NotInitialized` if called before `init()`.
3. **Name uniqueness**: Tool names must not collide with built-in ZeroClaw tools (e.g., `shell`, `file_read`, `web_fetch`).
4. **Timeout**: Default 30s per tool. Set `timeout_seconds` in `HostToolSpec` for custom values.
5. **Thread safety**: All functions are safe to call from any thread.
6. **Rebuild persistence**: Host tools survive config changes — no need to re-register after `update_config()`.
7. **Channel re-establishment**: Call `setup_tool_handler` again after the app resumes from background to re-attach a fresh request listener. Registered tools remain valid — no need to re-register.

## Channel Re-establishment (Mobile)

On mobile platforms, the app may be suspended by the OS. When it resumes, the previous handler stream is invalid. Call `setup_tool_handler` again to create a fresh channel:

```dart
// Flutter — resuming after background
class _MyAppState extends State<MyApp> with WidgetsBindingObserver {
  AgentHandle? _handle;

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed && _handle != null) {
      // Re-establish the handler channel — registered tools stay intact
      setupToolHandlerStream(_handle!, onToolRequest: (ToolRequest request) {
        _handleToolRequest(request);
      });
    }
  }
}
```

Any in-flight tool invocations from before the suspend receive a "channel closed" error. The agent handles this gracefully and can retry the tool call on the new channel.
