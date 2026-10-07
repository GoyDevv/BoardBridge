package com.xtg.pad;

import android.content.ComponentName;
import android.content.Context;
import android.content.ServiceConnection;
import android.content.pm.PackageManager;
import android.os.IBinder;
import rikka.shizuku.Shizuku;

/* Binds the privileged half of the app. Everything that can fail reports a readable
   reason, because on someone else's phone the only debugging tool is the text on screen. */
public final class ShizukuLink {

  public interface Listener { void onState(String text, boolean live); }

  private final Context ctx;
  private final Listener listener;
  private IVPad pad = null;
  private String state = "not started";

  public ShizukuLink(Context ctx, Listener l) { this.ctx = ctx; this.listener = l; }

  public boolean live() { return pad != null; }
  public IVPad pad() { return pad; }
  public String state() { return state; }

  private void set(String s) { state = s; if (listener != null) listener.onState(s, pad != null); }

  public void connect() {
    try {
      if (!Shizuku.pingBinder()) {
        set("Shizuku is not running. Open Shizuku and start it (wireless debugging or root), then try again.");
        return;
      }
    } catch (Throwable t) {
      set("Shizuku is not installed: " + t);
      return;
    }
    if (Shizuku.isPreV11()) { set("Shizuku is too old - v11 or newer is needed."); return; }
    try {
      if (Shizuku.checkSelfPermission() != PackageManager.PERMISSION_GRANTED) {
        set("waiting for permission in Shizuku...");
        Shizuku.addRequestPermissionResultListener(new Shizuku.OnRequestPermissionResultListener() {
          @Override public void onRequestPermissionResult(int code, int result) {
            Shizuku.removeRequestPermissionResultListener(this);
            if (result == PackageManager.PERMISSION_GRANTED) bind();
            else set("permission denied in Shizuku");
          }
        });
        Shizuku.requestPermission(1001);
        return;
      }
    } catch (Throwable t) { set("permission check failed: " + t); return; }
    bind();
  }

  private final ServiceConnection conn = new ServiceConnection() {
    @Override public void onServiceConnected(ComponentName n, IBinder b) {
      if (b == null || !b.pingBinder()) { set("user service did not start"); return; }
      pad = IVPad.Stub.asInterface(b);
      try {
        String e = pad.start();
        if (e != null) { set("could not create the controller: " + e); pad = null; return; }
        set("virtual Xbox controller is live (" + pad.diag() + ")");
      } catch (Throwable t) { pad = null; set("start failed: " + t); }
    }
    @Override public void onServiceDisconnected(ComponentName n) { pad = null; set("user service stopped"); }
  };

  private Shizuku.UserServiceArgs args() {
    return new Shizuku.UserServiceArgs(new ComponentName(ctx.getPackageName(), VPadService.class.getName()))
        .daemon(false)
        .processNameSuffix("vpad")
        .debuggable(false)
        .version(1);
  }

  private void bind() {
    try { Shizuku.bindUserService(args(), conn); set("starting the privileged helper..."); }
    catch (Throwable t) { set("bind failed: " + t); }
  }

  public void disconnect() {
    try { if (pad != null) pad.stop(); } catch (Throwable ignored) {}
    try { Shizuku.unbindUserService(args(), conn, true); } catch (Throwable ignored) {}
    pad = null;
    set("stopped");
  }
}
