package com.xtg.pad;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.Service;
import android.content.Intent;
import android.graphics.PixelFormat;
import android.os.Build;
import android.os.IBinder;
import android.view.Gravity;
import android.view.View;
import android.view.WindowManager;
import android.widget.Button;
import android.graphics.Color;

/* The point of the Shizuku path: the pad can sit on top of any app - Quetta, Chrome,
   the Xbox app - because the controller it drives is a real kernel input device rather
   than something injected into one page. */
public final class OverlayService extends Service {

  private WindowManager wm;
  private PadView view;
  private Button fab;
  private boolean padUp = false;
  private WindowManager.LayoutParams padLp;

  @Override public IBinder onBind(Intent i) { return null; }

  @Override public void onCreate() {
    super.onCreate();
    startForeground(7, note());
    Prefs p = new Prefs(this);
    boolean portrait = getResources().getConfiguration().orientation
        == android.content.res.Configuration.ORIENTATION_PORTRAIT;
    String profile = p.getStr("profile", "Default");
    view = new PadView(this, Core.state, p.loadLayout(profile, portrait));
    view.opacity = p.getInt("opacity", 55);
    view.lite = p.getBool("lite", false);
    view.hud = p.getBool("hud", false);
    view.floatStick = p.getBool("floatLs", true);
    view.autoHideS = p.getInt("autoHide", 0);
    boolean an = p.getBool("analogT", false);
    Ctrl lt = view.byId("lt"), rt = view.byId("rt");
    if (lt != null) lt.analog = an;
    if (rt != null) rt.analog = an;
    Core.state.density = getResources().getDisplayMetrics().density;
    Core.state.connected = true;
    Core.startPump();

    wm = (WindowManager) getSystemService(WINDOW_SERVICE);
    // Two windows, deliberately. A full screen overlay swallows every touch inside its
    // bounds - there is no way to hand an unclaimed one back to the app underneath - so
    // the pad lives in a window that can be taken away, and only a thumb sized button
    // stays on screen to bring it back. Everything outside that button belongs to
    // whatever app you are actually using.
    padLp = lp(WindowManager.LayoutParams.MATCH_PARENT, WindowManager.LayoutParams.MATCH_PARENT);
    fab = new Button(this);
    fab.setText("\u2b24");
    fab.setTextColor(Color.argb(210, 180, 225, 255));
    fab.setBackgroundColor(Color.argb(120, 10, 16, 24));
    fab.setOnClickListener(new View.OnClickListener() {
      @Override public void onClick(View v) { togglePad(); }
    });
    WindowManager.LayoutParams flp = lp(dp(44), dp(44));
    flp.gravity = Gravity.TOP | Gravity.END;
    try { wm.addView(fab, flp); } catch (Throwable t) { Core.note = "overlay: " + t; stopSelf(); return; }
    togglePad();
  }

  private int dp(int v) { return (int) (v * getResources().getDisplayMetrics().density); }

  private WindowManager.LayoutParams lp(int w, int h) {
    int type = Build.VERSION.SDK_INT >= 26
        ? WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY
        : WindowManager.LayoutParams.TYPE_PHONE;
    WindowManager.LayoutParams p = new WindowManager.LayoutParams(w, h, type,
        WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE
          | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN
          | WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS
          | WindowManager.LayoutParams.FLAG_WATCH_OUTSIDE_TOUCH
          | WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED,
        PixelFormat.TRANSLUCENT);
    p.gravity = Gravity.TOP | Gravity.START;
    if (Build.VERSION.SDK_INT >= 28) p.layoutInDisplayCutoutMode =
        WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
    return p;
  }

  private void togglePad() {
    try {
      if (padUp) { wm.removeView(view); padUp = false; fab.setAlpha(0.45f); }
      else { wm.addView(view, padLp); padUp = true; fab.setAlpha(1f); }
    } catch (Throwable t) { Core.note = "overlay: " + t; }
  }

  private Notification note() {
    String ch = "xtg";
    if (Build.VERSION.SDK_INT >= 26) {
      NotificationManager nm = (NotificationManager) getSystemService(NotificationManager.class);
      nm.createNotificationChannel(new NotificationChannel(ch, "Overlay", NotificationManager.IMPORTANCE_MIN));
      return new Notification.Builder(this, ch)
          .setContentTitle("XTG pad is on top")
          .setContentText("Virtual Xbox controller active")
          .setSmallIcon(android.R.drawable.ic_menu_compass)
          .build();
    }
    return new Notification.Builder(this)
        .setContentTitle("XTG pad is on top")
        .setSmallIcon(android.R.drawable.ic_menu_compass).build();
  }

  @Override public void onDestroy() {
    try { if (view != null && padUp) wm.removeView(view); } catch (Throwable ignored) {}
    try { if (fab != null) wm.removeView(fab); } catch (Throwable ignored) {}
    view = null; fab = null;
    super.onDestroy();
  }
}
