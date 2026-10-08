# goyxcomtrol 1.6.0 aim and latency patch

this branch contains a latency-focused patch for xtg cloud pad 1.1.0.

the patch keeps xcloud stream quality unchanged and targets the local input path:
- uses a short weighted touch-velocity window modeled on the original controller path
- uses one shared X/Y camera sensitivity and preserves the exact 53 ms stop threshold
- compensates xCloud's low-end stick deadzone while preserving aim direction
- requests unbuffered Android touch delivery for the camera zone
- uses a short 0.84 follow filter instead of a long camera history
- adds a compact allocation-light gamepad wire format
- preserves the legacy bridge as a fallback
- disables webview debugging in the normal build
- keeps lt/rt analog values intact in the fast path
