# goyxcomtrol 1.2.0 latency patch

this branch contains a latency-focused patch for xtg cloud pad 1.1.0.

the patch keeps xcloud stream quality unchanged and targets the local input path:
- replaces the 11-40 ms camera history scan with a newest-sample velocity estimator
- uses android uptime milliseconds consistently across touch, webview, and hid paths
- keeps the 53 ms stop threshold
- uses a short 0.84 follow filter instead of a long camera history
- adds a compact allocation-light gamepad wire format
- preserves the legacy bridge as a fallback
- disables webview debugging in the normal build
- keeps lt/rt analog values intact in the fast path
