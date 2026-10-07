package com.xtg.pad;

import org.json.JSONObject;

/* The layouts that ship. The Warzone one is carried over verbatim from the browser
   extension, where it was arrived at by actually playing: a left-thumb shoot pad, scope
   on the right, sprint and slide where the thumb already is. The extension stored a
   control's x,y as the CENTRE in percent and its size as a percent of the shorter screen
   edge, with zones stored as top-left plus width and height - this converts both. */
public final class Layouts {

  public static final String WZ_LAND =
    "{\"rs\":{\"x\":40,\"y\":0,\"w\":60,\"h\":100},\"lsz\":{\"x\":0,\"y\":39,\"w\":38,\"h\":61},"
    + "\"ls\":{\"x\":18.18,\"y\":70.04,\"s\":30},\"dpad\":{\"x\":6.17,\"y\":15.05,\"s\":26},"
    + "\"y\":{\"x\":65.85,\"y\":40.44,\"s\":14},\"x\":{\"x\":73.4,\"y\":39.87,\"s\":14},"
    + "\"b\":{\"x\":81.05,\"y\":40.46,\"s\":14},\"a\":{\"x\":68.25,\"y\":8.9,\"s\":14},"
    + "\"lt\":{\"x\":81.11,\"y\":9.13,\"s\":35,\"shape\":\"square\",\"lab\":\"scope\"},"
    + "\"lb\":{\"x\":19.83,\"y\":29.37,\"s\":17},"
    + "\"rt\":{\"x\":21.74,\"y\":10.4,\"s\":40,\"shape\":\"square\",\"lab\":\"Shoot\"},"
    + "\"rb\":{\"x\":29.5,\"y\":28.7,\"s\":17},\"view\":{\"x\":39,\"y\":6,\"s\":13},"
    + "\"xbox\":{\"x\":50,\"y\":6,\"s\":13},\"menu\":{\"x\":61,\"y\":6,\"s\":13},"
    + "\"l3\":{\"x\":78.39,\"y\":26.34,\"s\":20,\"shape\":\"square\",\"lab\":\"run\"},"
    + "\"r3\":{\"x\":68.11,\"y\":24.88,\"s\":21,\"lab\":\"Slide\"}}";

  public static final String WZ_PORT =
    "{\"rs\":{\"x\":0,\"y\":8,\"w\":100,\"h\":50},\"lsz\":{\"x\":0,\"y\":58,\"w\":52,\"h\":42},"
    + "\"ls\":{\"x\":22,\"y\":84,\"s\":34},\"dpad\":{\"x\":20,\"y\":66,\"s\":28},"
    + "\"y\":{\"x\":80,\"y\":68,\"s\":16},\"x\":{\"x\":67,\"y\":76,\"s\":16},"
    + "\"b\":{\"x\":92,\"y\":76,\"s\":16},\"a\":{\"x\":80,\"y\":84,\"s\":16},"
    + "\"lt\":{\"x\":10,\"y\":4,\"s\":19},\"lb\":{\"x\":32,\"y\":4,\"s\":19},"
    + "\"rt\":{\"x\":90,\"y\":4,\"s\":19},\"rb\":{\"x\":68,\"y\":4,\"s\":19},"
    + "\"view\":{\"x\":35,\"y\":61,\"s\":15},\"xbox\":{\"x\":50,\"y\":61,\"s\":15},"
    + "\"menu\":{\"x\":65,\"y\":61,\"s\":15},\"l3\":{\"x\":32,\"y\":95,\"s\":14},"
    + "\"r3\":{\"x\":68,\"y\":95,\"s\":14}}";

  /** turns one of the strings above into a live layout */
  public static Ctrl[] fromExtension(String json, boolean portrait) {
    Ctrl[] cs = Ctrl.defaults(portrait);
    try {
      JSONObject o = new JSONObject(json);
      for (Ctrl c : cs) {
        JSONObject p = o.optJSONObject(c.id);
        if (p == null) { c.on = false; continue; }
        if (c.kind == Ctrl.ZONE || c.kind == Ctrl.LZONE) {
          float w = (float) p.optDouble("w", 60) / 100f;
          float h = (float) p.optDouble("h", 100) / 100f;
          c.w = w; c.h = h;
          c.x = (float) p.optDouble("x", 40) / 100f + w / 2f;
          c.y = (float) p.optDouble("y", 0) / 100f + h / 2f;
        } else {
          c.x = (float) p.optDouble("x", 50) / 100f;
          c.y = (float) p.optDouble("y", 50) / 100f;
          c.w = (float) p.optDouble("s", 15) / 100f;
        }
        c.op = p.optInt("op", 100);
        c.square = "square".equals(p.optString("shape", "round"));
        String lab = p.optString("lab", null);
        if (lab != null && lab.length() > 0 && !"null".equals(lab)) c.lab = lab;
        c.on = !p.optBoolean("hidden", false);
      }
    } catch (Exception ignored) {}
    return cs;
  }

  public static Ctrl[] warzone(boolean portrait) {
    return fromExtension(portrait ? WZ_PORT : WZ_LAND, portrait);
  }

  public static final String[] NAMES = { "Default", "Warzone", "Saved A", "Saved B" };

  public static Ctrl[] preset(String name, boolean portrait) {
    if ("Warzone".equals(name)) return warzone(portrait);
    return Ctrl.defaults(portrait);
  }
}
