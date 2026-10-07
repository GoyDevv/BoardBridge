package com.xtg.pad;

import android.content.Context;
import android.content.res.Configuration;
import android.graphics.Color;
import android.graphics.drawable.GradientDrawable;
import android.os.Build;

/* Material You. On Android 12 and up the system exposes the palette it derived from the
   wallpaper as ordinary colour resources, so the app can use the real thing rather than
   an imitation of it. Older versions fall back to a fixed blue of the same shape. */
public final class Mat {

  public final boolean dark;
  public final int accent;      // primary
  public final int accentDim;
  public final int onAccent;
  public final int bg;          // sheet background
  public final int card;        // raised surface
  public final int cardHi;
  public final int text;
  public final int textDim;
  public final int outline;
  public final boolean dynamic;

  public Mat(Context c) {
    int mode = c.getResources().getConfiguration().uiMode & Configuration.UI_MODE_NIGHT_MASK;
    dark = mode != Configuration.UI_MODE_NIGHT_NO;
    boolean dyn = Build.VERSION.SDK_INT >= 31;
    int a = 0, ad = 0, n0 = 0, n1 = 0, n2 = 0;
    if (dyn) {
      try {
        a  = c.getColor(dark ? android.R.color.system_accent1_200 : android.R.color.system_accent1_600);
        ad = c.getColor(dark ? android.R.color.system_accent1_700 : android.R.color.system_accent1_100);
        n0 = c.getColor(dark ? android.R.color.system_neutral1_900 : android.R.color.system_neutral1_50);
        n1 = c.getColor(dark ? android.R.color.system_neutral1_800 : android.R.color.system_neutral1_100);
        n2 = c.getColor(dark ? android.R.color.system_neutral2_700 : android.R.color.system_neutral2_200);
      } catch (Throwable t) { dyn = false; }
    }
    dynamic = dyn;
    if (!dyn) {
      a  = dark ? 0xFF9FD4FF : 0xFF0B5FA5;
      ad = dark ? 0xFF14405E : 0xFFD5E8F8;
      n0 = dark ? 0xFF0E1216 : 0xFFF7F9FB;
      n1 = dark ? 0xFF171D23 : 0xFFEDF1F5;
      n2 = dark ? 0xFF222A32 : 0xFFDDE4EA;
    }
    accent = a; accentDim = ad; bg = n0; card = n1; cardHi = n2;
    onAccent = dark ? 0xFF10181F : 0xFFFFFFFF;
    text = dark ? 0xFFE6EDF3 : 0xFF11181F;
    textDim = dark ? 0xFF9BA7B4 : 0xFF5A6673;
    outline = dark ? 0x33FFFFFF : 0x22000000;
  }

  public GradientDrawable round(int color, float dp, float density) {
    GradientDrawable g = new GradientDrawable();
    g.setShape(GradientDrawable.RECTANGLE);
    g.setColor(color);
    g.setCornerRadius(dp * density);
    return g;
  }

  public GradientDrawable pill(int color, float density) { return round(color, 999f, density); }

  public static int alpha(int c, int a) {
    return Color.argb(a, Color.red(c), Color.green(c), Color.blue(c));
  }
}
