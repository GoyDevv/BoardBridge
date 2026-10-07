package com.xtg.pad;

import android.os.SystemClock;
import android.webkit.JavascriptInterface;

/* Minimal synchronous bridge. Browser gamepad polling is latency-sensitive, so the hot
   path returns one compact UTF-16 snapshot instead of a comma-separated structure. */
public final class WebBridge {
  private final PadState st;
  public WebBridge(PadState st) { this.st = st; }

  @JavascriptInterface
  public String stateFast() {
    st.camCompute(SystemClock.uptimeMillis());
    return st.fastWire();
  }

  @JavascriptInterface
  public String state() {
    st.camCompute(SystemClock.uptimeMillis());
    return st.wire();
  }

  @JavascriptInterface
  public String kbm() {
    return st.kbm ? st.kbmTake() : null;
  }
}
