package com.xtg.pad;

import android.system.Os;
import android.system.OsConstants;
import java.io.FileDescriptor;

/* A real controller, created by the kernel, in pure Java.

   /dev/uhid is a write-only protocol: you write a packed uhid_event to create the
   device and another one for every input report. It needs no ioctl at all, which is
   the whole reason this can be done without any native code. /dev/uinput, by contrast,
   is driven entirely by ioctl and would need JNI.

   Who is allowed to open it: /dev/uinput and /dev/uhid are both labelled uhid_device,
   system/sepolicy grants "allow shell uhid_device:chr_file rw_file_perms", the nodes
   are mode 0660 owner uhid group uhid, and adbd adds AID_UHID to the shell process's
   supplementary groups specifically so the shell "hid" command can use them. A Shizuku
   user service is forked from that shell, so it inherits exactly those rights. A normal
   app process has none of them, which is why this class only ever runs on the far side
   of Shizuku.

   The device identifies itself as a Microsoft 045E:02EA, so Android loads its built-in
   Vendor_045e_Product_02ea keylayout and every app, including Chromium and the Xbox
   app, sees a genuine Xbox Wireless Controller with analog triggers. */
public final class UHid {

  private static final int UHID_DESTROY = 1, UHID_CREATE2 = 11, UHID_INPUT2 = 12;
  private static final int BUS_BLUETOOTH = 0x05;

  /* Report: X Y Rx Ry as left/right stick, Z and Rz as the analog triggers, a hat
     switch for the d-pad, 15 buttons. The axis choice is not arbitrary - the AOSP
     keylayout for this vendor maps ABS_Z/ABS_RZ to LTRIGGER/RTRIGGER and ABS_RX/ABS_RY
     to the right stick, so the descriptor has to agree with it. */
  static final byte[] RD = {
    0x05, 0x01,                   // Usage Page (Generic Desktop)
    0x09, 0x05,                   // Usage (Game Pad)
    (byte) 0xA1, 0x01,            // Collection (Application)
    (byte) 0xA1, 0x00,            //   Collection (Physical)
    0x09, 0x30,                   //     Usage (X)
    0x09, 0x31,                   //     Usage (Y)
    0x09, 0x33,                   //     Usage (Rx)
    0x09, 0x34,                   //     Usage (Ry)
    0x16, 0x00, 0x00,             //     Logical Minimum (0)
    0x27, (byte) 0xFF, (byte) 0xFF, 0x00, 0x00,  // Logical Maximum (65535)
    0x75, 0x10,                   //     Report Size (16)
    (byte) 0x95, 0x04,            //     Report Count (4)
    (byte) 0x81, 0x02,            //     Input (Data,Var,Abs)
    (byte) 0xC0,                  //   End Collection
    0x09, 0x32,                   //   Usage (Z)   -> left trigger
    0x09, 0x35,                   //   Usage (Rz)  -> right trigger
    0x16, 0x00, 0x00,             //   Logical Minimum (0)
    0x27, (byte) 0xFF, (byte) 0xFF, 0x00, 0x00,
    0x75, 0x10,                   //   Report Size (16)
    (byte) 0x95, 0x02,            //   Report Count (2)
    (byte) 0x81, 0x02,            //   Input (Data,Var,Abs)
    0x05, 0x01,                   //   Usage Page (Generic Desktop)
    0x09, 0x39,                   //   Usage (Hat switch)
    0x15, 0x01,                   //   Logical Minimum (1)
    0x25, 0x08,                   //   Logical Maximum (8)
    0x35, 0x00,                   //   Physical Minimum (0)
    0x46, 0x3B, 0x01,             //   Physical Maximum (315)
    0x65, 0x14,                   //   Unit (degrees)
    0x75, 0x04,                   //   Report Size (4)
    (byte) 0x95, 0x01,            //   Report Count (1)
    (byte) 0x81, 0x42,            //   Input (Data,Var,Abs,Null State)
    0x65, 0x00,                   //   Unit (none)
    0x75, 0x04,                   //   Report Size (4)
    (byte) 0x95, 0x01,            //   Report Count (1)
    (byte) 0x81, 0x03,            //   Input (Cnst,Var,Abs)  - padding
    0x05, 0x09,                   //   Usage Page (Button)
    0x19, 0x01,                   //   Usage Minimum (Button 1)
    0x29, 0x0F,                   //   Usage Maximum (Button 15)
    0x15, 0x00,                   //   Logical Minimum (0)
    0x25, 0x01,                   //   Logical Maximum (1)
    0x75, 0x01,                   //   Report Size (1)
    (byte) 0x95, 0x0F,            //   Report Count (15)
    (byte) 0x81, 0x02,            //   Input (Data,Var,Abs)
    0x75, 0x01,                   //   Report Size (1)
    (byte) 0x95, 0x01,            //   Report Count (1)
    (byte) 0x81, 0x03,            //   Input (Cnst,Var,Abs)  - padding
    (byte) 0xC0                   // End Collection
  };

  private FileDescriptor fd = null;
  private String err = null;

  public String open() {
    if (fd != null) return null;
    try {
      fd = Os.open("/dev/uhid", OsConstants.O_RDWR, 0);
    } catch (Throwable t) {
      fd = null;
      err = "open /dev/uhid failed: " + t;
      return err;
    }
    try {
      Os.write(fd, create2(), 0, create2Len);
    } catch (Throwable t) {
      err = "UHID_CREATE2 failed: " + t;
      close();
      return err;
    }
    return null;
  }

  private byte[] c2 = null; private int create2Len = 0;
  private byte[] create2() {
    if (c2 != null) return c2;
    // struct uhid_event { u32 type; struct uhid_create2_req { u8 name[128]; u8 phys[64];
    //   u8 uniq[64]; u16 rd_size; u16 bus; u32 vendor; u32 product; u32 version;
    //   u32 country; u8 rd_data[4096]; } } __packed
    // The kernel zero fills its buffer before copying, so a write truncated right after
    // the descriptor is both legal and all we need.
    int head = 4 + 128 + 64 + 64 + 2 + 2 + 4 + 4 + 4 + 4;
    byte[] b = new byte[head + RD.length];
    le32(b, 0, UHID_CREATE2);
    str(b, 4, "Xbox Wireless Controller", 128);
    str(b, 4 + 128, "xtg", 64);
    str(b, 4 + 128 + 64, "xtg-virtual-pad", 64);
    int p = 4 + 128 + 64 + 64;
    le16(b, p, RD.length); le16(b, p + 2, BUS_BLUETOOTH);
    le32(b, p + 4, 0x045E);       // Microsoft
    le32(b, p + 8, 0x02EA);       // Xbox Wireless Controller
    le32(b, p + 12, 0x0903);
    le32(b, p + 16, 0);
    System.arraycopy(RD, 0, b, head, RD.length);
    c2 = b; create2Len = b.length;
    return b;
  }

  private final byte[] inBuf = new byte[4 + 2 + 64];
  public String input(byte[] report) {
    if (fd == null) return "not open";
    try {
      le32(inBuf, 0, UHID_INPUT2);
      le16(inBuf, 4, report.length);
      System.arraycopy(report, 0, inBuf, 6, report.length);
      Os.write(fd, inBuf, 0, 6 + report.length);
      return null;
    } catch (Throwable t) { err = "write failed: " + t; return err; }
  }

  public void close() {
    if (fd == null) return;
    try { byte[] b = new byte[4]; le32(b, 0, UHID_DESTROY); Os.write(fd, b, 0, 4); } catch (Throwable ignored) {}
    try { Os.close(fd); } catch (Throwable ignored) {}
    fd = null;
  }

  public boolean isOpen() { return fd != null; }
  public String lastError() { return err; }

  private static void le16(byte[] b, int i, int v) { b[i] = (byte) v; b[i + 1] = (byte) (v >> 8); }
  private static void le32(byte[] b, int i, int v) {
    b[i] = (byte) v; b[i + 1] = (byte) (v >> 8); b[i + 2] = (byte) (v >> 16); b[i + 3] = (byte) (v >> 24);
  }
  private static void str(byte[] b, int off, String s, int len) {
    byte[] x = s.getBytes();
    int n = Math.min(x.length, len - 1);
    System.arraycopy(x, 0, b, off, n);
  }
}
