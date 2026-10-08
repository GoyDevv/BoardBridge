package com.goydevv.inputbridge;

import android.app.Activity;
import android.content.Intent;
import android.graphics.Color;
import android.os.Bundle;
import android.provider.Settings;
import android.view.Gravity;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;

public final class MainActivity extends Activity {
    private BridgeClient bridge;
    private TextView status;

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        bridge=BridgeClient.get(this);
        build();
        refresh();
    }

    @Override protected void onResume() {
        super.onResume();
        if (status != null) refresh();
    }

    private void build() {
        LinearLayout root=new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setPadding(dp(18),dp(24),dp(18),dp(24));
        root.setBackgroundColor(Color.rgb(10,10,14));

        TextView title=text("XCLOUD INPUT BRIDGE",28,Color.WHITE);
        title.setGravity(Gravity.CENTER_VERTICAL);
        root.addView(title,new LinearLayout.LayoutParams(-1,dp(58)));

        TextView sub=text("Shizuku • relative mouse • keyboard",13,Color.LTGRAY);
        root.addView(sub,new LinearLayout.LayoutParams(-1,dp(40)));

        status=text("",12,Color.LTGRAY);
        root.addView(status,new LinearLayout.LayoutParams(-1,dp(90)));

        Button shizuku=button("OPEN SHIZUKU SETTINGS");
        shizuku.setOnClickListener(v->{try{startActivity(new Intent("moe.shizuku.manager.intent.action.MANAGER"));}catch(Exception e){Toast.makeText(this,"Open Shizuku manually",Toast.LENGTH_SHORT).show();}});
        root.addView(shizuku);

        Button overlay=button("ALLOW DRAW OVER OTHER APPS");
        overlay.setOnClickListener(v->startActivity(new Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION,android.net.Uri.parse("package:"+getPackageName()))));
        root.addView(overlay);

        Button start=button("START INPUT OVERLAY");
        start.setOnClickListener(v->startBridge());
        root.addView(start);

        Button stop=button("STOP INPUT OVERLAY");
        stop.setOnClickListener(v->{stopService(new Intent(this,OverlayService.class));bridge.releaseAll();refresh();});
        root.addView(stop);

        Button editor=button("CUSTOM CONTROLS");
        editor.setOnClickListener(v->startActivity(new Intent(this,ControlEditorActivity.class)));
        root.addView(editor);

        ScrollView scroll=new ScrollView(this);
        TextView info=text(
            "\nRIGHT SIDE = RELATIVE MOUSE\n"
            +"No dead-zone, no smoothing, no integer rounding.\n\n"
            +"Use the editor to add buttons, joystick, d-pad and drawers.\n"
            +"Up to four input actions can be stored on each control.",
            13,Color.rgb(185,185,195));
        info.setPadding(0,dp(12),0,dp(12));
        scroll.addView(info);
        root.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));

        setContentView(root);
    }

    private void startBridge() {
        if (!bridge.isShizukuRunning()) {toast("Shizuku is not running");return;}
        if (!bridge.hasPermission()) {bridge.requestPermission();toast("Grant Shizuku permission, then tap start again");return;}
        if (!Settings.canDrawOverlays(this)) {toast("Allow overlay permission first");return;}
        if (!bridge.startService()) {toast("Could not bind Shizuku user service");return;}
        Intent i=new Intent(this,OverlayService.class);
        if(android.os.Build.VERSION.SDK_INT>=26)startForegroundService(i);else startService(i);
        toast("Overlay started");
        refresh();
    }

    private void refresh() {
        boolean s=bridge.isShizukuRunning(), p=bridge.hasPermission(), o=Settings.canDrawOverlays(this);
        status.setText("Shizuku: "+(s?"OK":"MISSING")+"\nPermission: "+(p?"OK":"MISSING")+"\nOverlay: "+(o?"OK":"MISSING")+"\nInjector: "+bridge.status()+"\nUID: "+bridge.uid());
    }

    private Button button(String s){Button b=new Button(this);b.setText(s);b.setAllCaps(false);return b;}
    private TextView text(String s,int size,int color){TextView t=new TextView(this);t.setText(s);t.setTextSize(size);t.setTextColor(color);return t;}
    private int dp(int n){return Math.round(n*getResources().getDisplayMetrics().density);}
    private void toast(String s){Toast.makeText(this,s,Toast.LENGTH_SHORT).show();}
}
