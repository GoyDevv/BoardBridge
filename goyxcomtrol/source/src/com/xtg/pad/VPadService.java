package com.xtg.pad;

import android.os.Process;

/* Runs inside Shizuku's user service, so as uid 2000 (shell) or 0 (root), with the
   supplementary groups and SELinux context that let it touch /dev/uhid. The app process
   itself can do none of this; it just makes binder calls across. */
public class VPadService extends IVPad.Stub {

  private final UHid hid = new UHid();
  private String note = "";

  public VPadService() { }

  @Override public String start() {
    note = "uid=" + Process.myUid();
    String e = hid.open();
    if (e != null) { note += " " + e; return e; }
    note += " device created";
    return null;
  }

  @Override public void report(byte[] r) {
    if (r != null) hid.input(r);
  }

  @Override public void stop() { hid.close(); }

  @Override public String diag() {
    return note + (hid.isOpen() ? " [open]" : " [closed]")
        + (hid.lastError() != null ? " last=" + hid.lastError() : "");
  }

  @Override public void destroy() {
    hid.close();
    System.exit(0);
  }
}
