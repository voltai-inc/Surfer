// Web apps which integrate Surfer as an iframe can give commands to surfer via
// the .postMessage [1] function on the iframe.
//
//  For example, to tell Surfer to load waveforms from a URL, use
// `.postMessage({command: "LoadUrl", url: "https://app.surfer-project.org/picorv32.vcd"})`
//
//  For more complex functionality, one can also inject any `Message` defined
// in `surfer::Message` in surfer/main.rs. However, the API of these messages
// is not stable and may change at any time. If you add functionality via
// these, make sure to test the new functionality when changing Surfer version.
//
// [1] https://developer.mozilla.org/en-US/docs/Web/API/Window/postMessage

function register_message_listener() {
  window.addEventListener("message", (event) => {
    // JSON decode the message
    const decoded = event.data

    switch (decoded.command) {
      // Load a waveform from a URL. The format is inferred from the data.
      // Example: `{command: "LoadUrl", url: "https://app.surfer-project.org/picorv32.vcd"}`

      case 'LoadUrl': {
        const msg = {
          LoadWaveformFileFromUrl: [
            decoded.url,
            { keep_variables: false, keep_unavailable: false, expect_format: null }
          ]
        }
        inject_message(JSON.stringify(msg))
        break;
      }

      case 'ToggleMenu': {
        const msg = "ToggleMenu"
        inject_message(JSON.stringify(msg))
        break;
      }

      // Inject any other message supported by Surfer in the surfer::Message enum.
      // NOTE: The API of these is unstable.
      case 'InjectMessage': {
        inject_message(decoded.message);
        break
      }

      // Send WCP message through the established WCP channel
      case 'SendWcpMessage': {
        console.log('📤 Sending WCP message to Surfer:', decoded.message);

        // Send WCP client message through WASM if available
        if (window.handle_wcp_cs_message) {
          try {
            window.handle_wcp_cs_message(decoded.message);
            console.log('✅ WCP message sent successfully');
          } catch (error) {
            console.error('❌ Failed to send WCP message:', error);
          }
        } else {
          console.warn('⚠️ handle_wcp_cs_message not available yet');
        }
        break;
      }

      default:
        console.log(`Unknown message.command ${decoded.command}`)
        break;
    }
  });
}

// WCP Integration: Forward WCP messages from Surfer to parent window
window.send_wcp_to_vscode = function(wcpMessage) {
  if (window.parent) {
    window.parent.postMessage({
      command: 'WcpMessage',
      data: wcpMessage
    }, '*');
  }
};
