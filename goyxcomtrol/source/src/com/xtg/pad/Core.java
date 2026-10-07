package com.xtg.pad;

/* One controller per process, shared by the activity and the system overlay service. */
import android.os.SystemClock;
public final class Core {
  public static final PadState state = new PadState();
  public static ShizukuLink link = null;
  private static Thread pump = null;
  private static volatile boolean run = false;
  public static volatile String note = "";

  /** Pushes HID reports to the privileged helper. 250Hz, and only when something
      actually changed, so an idle pad costs nothing. */
  public static synchronized void startPump() {
    if (run) return;
    run = true;
    pump = new Thread(new Runnable() {
      @Override public void run() {
        byte[] prev = new byte[15];
        while (run) {
          try {
            ShizukuLink l = link;
            if (l != null && l.live()) {
              state.camCompute(SystemClock.uptimeMillis());
              byte[] r = state.report();
              boolean diff = false;
              for (int i = 0; i < r.length; i++) if (r[i] != prev[i]) { diff = true; break; }
              if (diff) {
                System.arraycopy(r, 0, prev, 0, r.length);
                l.pad().report(r);
              }
            }
            Thread.sleep(0, 4000000);
          } catch (Throwable t) { note = "pump: " + t; }
        }
      }
    }, "xtg-pump");
    pump.setPriority(Thread.MAX_PRIORITY);
    pump.start();
  }

  public static synchronized void stopPump() { run = false; pump = null; }
}
