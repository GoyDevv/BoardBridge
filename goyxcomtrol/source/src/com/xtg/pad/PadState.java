package com.xtg.pad;

/* The single source of truth for what the virtual controller is reporting.
   Touch handling writes into it, and both consumers read from it:
   - WebView mode: the injected JavaScript pulls a compact state snapshot
   - Shizuku mode: a 15 byte HID report is built from it

   The camera path intentionally avoids a long history window. We estimate velocity from
   the newest digitiser sample, apply only a very small one-pole filter, and let the
   stop threshold decide how long that velocity may survive a late web-frame. */
public final class PadState {

  public static final int A = 0, B = 1, X = 2, Y = 3, LB = 4, RB = 5, LT = 6, RT = 7,
      VIEW = 8, MENU = 9, L3 = 10, R3 = 11, DU = 12, DD = 13, DL = 14, DR = 15, GUIDE = 16;
  public static final int NB = 17;

  public final boolean[] btn = new boolean[NB];
  public volatile float lx, ly, rx, ry;     // -1..1
  public volatile float lt, rt;             // 0..1, analog
  public volatile boolean connected;

  /* ---- tunables, persisted ---- */
  public volatile int sens = 175;           // unified camera sensitivity, % (X and Y together)
  public volatile int stillMs = 53;         // user's sweet spot: late-frame coast threshold
  public volatile boolean invertY = false;
  public volatile int deadzone = 0;         // left stick, %
  public volatile float density = 2.75f;    // px per dp, so sensitivity means the same thing
                                            // on any screen
  public volatile boolean kbm = false;      // send mouse and keys to the page instead
  public volatile int mouseSens = 140;      // % of 1:1 for mouse mode
  public volatile long lastActive = 0;      // for auto-hide

  /* ---- mouse + keyboard queue, drained by the page -----------------------------
     In mouse mode the camera is not a stick at all: the pixels your thumb moved become
     the same number of mouse pixels. There is no deflection, so no cap on turn speed and
     no rate for the game to integrate. The queue exists only because the page has to do the
     dispatching, and it is drained whole on every read. */
  public static final String[] KBM_DEF = new String[NB];
  static {
    KBM_DEF[A] = "k:Space";      KBM_DEF[B] = "k:ControlLeft";
    KBM_DEF[X] = "k:KeyR";       KBM_DEF[Y] = "k:KeyF";
    KBM_DEF[LB] = "k:KeyQ";      KBM_DEF[RB] = "k:KeyG";
    KBM_DEF[LT] = "b:2";         KBM_DEF[RT] = "b:0";
    KBM_DEF[VIEW] = "k:Tab";     KBM_DEF[MENU] = "k:Escape";  KBM_DEF[GUIDE] = "k:Escape";
    KBM_DEF[L3] = "k:ShiftLeft"; KBM_DEF[R3] = "k:KeyV";
    KBM_DEF[DU] = "k:ArrowUp";   KBM_DEF[DD] = "k:ArrowDown";
    KBM_DEF[DL] = "k:ArrowLeft"; KBM_DEF[DR] = "k:ArrowRight";
  }
  private final StringBuilder q = new StringBuilder(256);
  private float mdx = 0, mdy = 0;
  private final boolean[] wasd = new boolean[4];
  private static final String[] WASD = { "KeyW", "KeyS", "KeyA", "KeyD" };

  public synchronized void kbmMouse(float dx, float dy) {
    float g = mouseSens / 100f;
    mdx += dx * g;
    mdy += dy * g * (invertY ? -1 : 1);
  }

  public synchronized void kbmSend(int idx, boolean down) {
    String b = KBM_DEF[idx];
    if (b == null) return;
    if (q.length() > 0) q.append('|');
    q.append(b.charAt(0) == 'k' ? 'k' : 'b').append(down ? '+' : '-').append(b.substring(2));
  }

  /** left stick becomes WASD, pressed and released only on a real change of direction */
  public synchronized void kbmStick(float ax, float ay) {
    float t = 0.38f;
    boolean[] want = { ay < -t, ay > t, ax < -t, ax > t };
    for (int i = 0; i < 4; i++) {
      if (want[i] == wasd[i]) continue;
      wasd[i] = want[i];
      if (q.length() > 0) q.append('|');
      q.append('k').append(want[i] ? '+' : '-').append(WASD[i]);
    }
  }

  public synchronized void kbmReleaseAll() {
    for (int i = 0; i < 4; i++) wasd[i] = false;
    if (q.length() > 0) q.append('|');
    q.append('r');
  }

  /** "<dx>,<dy>|cmd|cmd" and the queue is emptied */
  public synchronized String kbmTake() {
    String out = ((int) mdx) + "," + ((int) mdy) + (q.length() > 0 ? "|" + q : "");
    mdx -= (int) mdx;
    mdy -= (int) mdy;
    q.setLength(0);
    return out;
  }

  public float touchInterval() { return interval; }

  /* ---- camera ---------------------------------------------------------------
     Relative touch camera. Every digitiser sample is converted immediately to a
     frame-rate-independent velocity. There is no history window and no delayed
     renderer queue. A very small asymmetric filter only suppresses digitiser noise;
     reversals are applied immediately. The 53 ms value remains the hard stop watchdog. */

  private static final float AIM_DEADZONE = 0.08f;
  private static final float AIM_NOISE_PX = 0.22f;

  private long prevT = 0;
  private long lastMove = 0;
  private float interval = 8f;
  private float vx = 0f, vy = 0f;
  private boolean camDown = false;

  public void camDown(long tMs) {
    camDown = true;
    prevT = tMs;
    lastMove = tMs;
    vx = vy = 0f;
    rx = ry = 0f;
  }

  public void camSample(float dx, float dy, long tMs) {
    if (!camDown) return;

    long dtMs = tMs - prevT;
    if (dtMs <= 0) dtMs = 1;
    prevT = tMs;

    if (dtMs <= 60L) interval += (dtMs - interval) * 0.20f;

    if (dtMs > 120L) {
      vx = vy = 0f;
      rx = ry = 0f;
      lastMove = tMs;
      return;
    }

    lastMove = tMs;

    /* Ignore sub-pixel digitiser chatter only when it is genuinely tiny and
       arrives inside a very short sample interval. Real micro-aim is preserved. */
    if (dtMs <= 8L && Math.abs(dx) + Math.abs(dy) < AIM_NOISE_PX) {
      dx = dy = 0f;
    }

    float dens = density <= 0f ? 1f : density;
    float invDt = 1000f / (float) dtMs;

    /* Match the proven extension scale: 150 is the neutral reference.
       One sensitivity value drives both axes, so circles stay circles. */
    float gain = sens / 150f;
    float rawX = (dx / dens) * invDt * gain;
    float rawY = (dy / dens) * invDt * gain;
    if (invertY) rawY = -rawY;

    /* Very light low-pass filtering. Direction changes bypass the filter so
       flicks and tracking reversals do not feel delayed. */
    float dot = rawX * vx + rawY * vy;
    float alpha;
    if (dot < -0.0002f) {
      alpha = 1f;
    } else {
      float rawMag2 = rawX * rawX + rawY * rawY;
      float oldMag2 = vx * vx + vy * vy;
      alpha = rawMag2 >= oldMag2 ? 0.82f : 0.90f;
    }

    vx += (rawX - vx) * alpha;
    vy += (rawY - vy) * alpha;

    float outX = vx;
    float outY = vy;
    float m = (float) Math.sqrt(outX * outX + outY * outY);

    if (m > 1f) {
      float k = 1f / m;
      outX *= k;
      outY *= k;
      m = 1f;
    }

    /* Compensate the game's low-end stick dead zone without changing direction.
       This keeps slow aim alive while avoiding a jump when the finger first moves. */
    if (m > 0f && m < AIM_DEADZONE) {
      float k = AIM_DEADZONE / m;
      outX *= k;
      outY *= k;
    } else if (m == 0f) {
      outX = outY = 0f;
    }

    rx = clamp(outX);
    ry = clamp(outY);
  }

  public void camUp() {
    camDown = false;
    rx = ry = 0f;
    vx = vy = 0f;
  }

  public void camCompute(long nowMs) {
    if (!camDown) {
      rx = ry = 0f;
      return;
    }

    long age = nowMs - lastMove;
    if (age >= stillMs) {
      /* 53 ms stays the exact hard cutoff. */
      rx = ry = 0f;
      vx = vy = 0f;
      return;
    }

    /* Do not hold full stick strength for the whole watchdog window.
       Fade quickly after the last sample, which removes visible coast/drift
       without shortening the requested 53 ms safety threshold. */
    float factor = 1f;
    if (age > 7L) {
      float u = (age - 7f) / Math.max(1f, stillMs - 7f);
      factor = 1f - u;
      factor *= factor;
    }

    float outX = vx * factor;
    float outY = vy * factor;
    float m = (float) Math.sqrt(outX * outX + outY * outY);
    if (m > 1f) {
      float k = 1f / m;
      outX *= k;
      outY *= k;
    }
    rx = clamp(outX);
    ry = clamp(outY);
  }

  private static float clamp(float v) {
    return v < -1f ? -1f : (v > 1f ? 1f : v);
  }

  /* ---- legacy wire format for compatibility/debug -------------------------------- */
  private final StringBuilder sb = new StringBuilder(160);
  public String wire() {
    sb.setLength(0);
    sb.append(connected ? '1' : '0').append(';');
    sb.append((int) (lx * 1000)).append(',').append((int) (ly * 1000)).append(',')
      .append((int) (rx * 1000)).append(',').append((int) (ry * 1000)).append(';');
    for (int i = 0; i < NB; i++) {
      if (i > 0) sb.append(',');
      float v = btn[i] ? 1f : 0f;
      if (i == LT) v = lt; else if (i == RT) v = rt;
      sb.append((int) (v * 1000));
    }
    return sb.toString();
  }

  /* ---- compact WebView snapshot ----------------------------------------------------
     9 UTF-16 code units:
       0 connected
       1..4 axes, 12-bit unsigned each
       5..6 LT/RT, 10-bit unsigned each
       7..8 button bits, low 16 + high 1
     This avoids JS split()/substring()/number-array allocations on every poll. */
  private final char[] fast = new char[9];
  private String fastCache = null;
  private int fastC0 = -1, fastC1 = -1, fastC2 = -1, fastC3 = -1;
  private int fastT0 = -1, fastT1 = -1, fastBits0 = -1, fastBits1 = -1;
  private int fastConn = -1;

  public String fastWire() {
    int c0 = connected ? 1 : 0;
    int a0 = axis12(lx), a1 = axis12(ly), a2 = axis12(rx), a3 = axis12(ry);
    int t0 = trigger10(lt), t1 = trigger10(rt);
    int bits0 = 0, bits1 = 0;
    for (int i = 0; i < 16; i++) if (btn[i]) bits0 |= 1 << i;
    if (btn[16]) bits1 = 1;

    // xCloud polls getGamepads() repeatedly. Reuse the immutable snapshot while
    // the quantized state is unchanged, avoiding a Java String allocation on every
    // poll and reducing GC pressure during video playback.
    if (fastCache != null && fastConn == c0 && fastC0 == a0 && fastC1 == a1
        && fastC2 == a2 && fastC3 == a3 && fastT0 == t0 && fastT1 == t1
        && fastBits0 == bits0 && fastBits1 == bits1) {
      return fastCache;
    }

    fast[0] = (char) c0;
    fast[1] = (char) a0;
    fast[2] = (char) a1;
    fast[3] = (char) a2;
    fast[4] = (char) a3;
    fast[5] = (char) t0;
    fast[6] = (char) t1;
    fast[7] = (char) bits0;
    fast[8] = (char) bits1;

    fastC0 = a0; fastC1 = a1; fastC2 = a2; fastC3 = a3;
    fastT0 = t0; fastT1 = t1; fastBits0 = bits0; fastBits1 = bits1; fastConn = c0;
    fastCache = new String(fast);
    return fastCache;
  }

  private static int axis12(float v) {
    float c = clamp(v);
    int o = Math.round((c + 1f) * 2047.5f);
    return o < 0 ? 0 : (o > 4095 ? 4095 : o);
  }

  private static int trigger10(float v) {
    float c = clamp01(v);
    int o = Math.round(c * 1023f);
    return o < 0 ? 0 : (o > 1023 ? 1023 : o);
  }

  /* ---- HID report, 15 bytes, laid out to match the descriptor in UHid ------------
     X Y Rx Ry Z Rz as 16 bit unsigned, hat nibble, then 15 button bits. */
  private final byte[] rep = new byte[15];
  public byte[] report() {
    put16(0, axis16(lx)); put16(2, axis16(ly));
    put16(4, axis16(rx)); put16(6, axis16(ry));
    put16(8, (int) (clamp01(lt) * 65535f)); put16(10, (int) (clamp01(rt) * 65535f));
    boolean u = btn[DU], d = btn[DD], l = btn[DL], r = btn[DR];
    int hat = 0;
    if (u && r) hat = 2; else if (r && d) hat = 4; else if (d && l) hat = 6;
    else if (l && u) hat = 8; else if (u) hat = 1; else if (r) hat = 3;
    else if (d) hat = 5; else if (l) hat = 7;
    rep[12] = (byte) hat;
    int b = 0;
    if (btn[A]) b |= 1 << 0;
    if (btn[B]) b |= 1 << 1;
    if (btn[X]) b |= 1 << 3;
    if (btn[Y]) b |= 1 << 4;
    if (btn[LB]) b |= 1 << 6;
    if (btn[RB]) b |= 1 << 7;
    if (btn[VIEW]) b |= 1 << 10;
    if (btn[MENU]) b |= 1 << 11;
    if (btn[GUIDE]) b |= 1 << 12;
    if (btn[L3]) b |= 1 << 13;
    if (btn[R3]) b |= 1 << 14;
    rep[13] = (byte) (b & 0xFF);
    rep[14] = (byte) ((b >> 8) & 0x7F);
    return rep;
  }

  private static float clamp01(float v) { return v < 0 ? 0 : (v > 1 ? 1 : v); }
  private static int axis16(float v) {
    int o = Math.round((clamp(v) + 1f) * 32767.5f);
    return o < 0 ? 0 : (o > 65535 ? 65535 : o);
  }
  private void put16(int i, int v) { rep[i] = (byte) (v & 0xFF); rep[i + 1] = (byte) ((v >> 8) & 0xFF); }
}
