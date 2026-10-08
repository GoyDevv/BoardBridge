package com.goydevv.inputbridge;

import android.app.Activity;
import android.content.Intent;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.net.Uri;
import android.os.Bundle;
import android.provider.Settings;
import android.view.Gravity;
import android.view.View;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;

public final class MainActivity extends Activity {
    private BridgeClient bridge;
    private TextView status;
    private TextView badge;

    private final int BG=Color.rgb(9,10,14);
    private final int CARD=Color.rgb(20,22,29);
    private final int TEXT=Color.rgb(244,245,248);
    private final int MUTED=Color.rgb(157,162,174);
    private final int ACCENT=Color.rgb(115,92,255);

    @Override public void onCreate(Bundle state){
        super.onCreate(state);
        bridge=BridgeClient.get(this);
        build();
        refresh();
    }

    @Override protected void onResume(){
        super.onResume();
        if(status!=null)refresh();
    }

    private void build(){
        ScrollView scroll=new ScrollView(this);
        scroll.setFillViewport(true);
        scroll.setBackgroundColor(BG);

        LinearLayout root=new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setPadding(dp(20),dp(22),dp(20),dp(28));
        scroll.addView(root);

        TextView eyebrow=text("GOYDEVV  /  INPUT",12,ACCENT);
        eyebrow.setTypeface(Typeface.DEFAULT,Typeface.BOLD);
        root.addView(eyebrow);

        TextView title=text("Xcloud Input Bridge",30,TEXT);
        title.setTypeface(Typeface.DEFAULT,Typeface.BOLD);
        root.addView(title,lp(1,dp(42)));

        TextView subtitle=text("Low-latency Shizuku keyboard + relative mouse for cloud gaming.",14,MUTED);
        root.addView(subtitle,lp(1,dp(46)));

        LinearLayout statusCard=card();
        LinearLayout statusRow=new LinearLayout(this);
        statusRow.setGravity(Gravity.CENTER_VERTICAL);
        badge=text("CHECKING",11,TEXT);
        badge.setGravity(Gravity.CENTER);
        badge.setTypeface(Typeface.DEFAULT,Typeface.BOLD);
        statusRow.addView(badge,badgeLp());
        TextView live=text(" Bridge status",15,TEXT);
        live.setTypeface(Typeface.DEFAULT,Typeface.BOLD);
        statusRow.addView(live);
        statusCard.addView(statusRow);
        status=text("",13,MUTED);
        status.setPadding(0,dp(12),0,0);
        statusCard.addView(status);
        root.addView(statusCard,lp(1,dp(150)));

        TextView setup=text("SETUP",12,MUTED);
        setup.setTypeface(Typeface.DEFAULT,Typeface.BOLD);
        root.addView(setup,lp(1,dp(34)));

        root.addView(action("1", "Shizuku permission", "Start or grant the injector service.", v->openShizuku()));
        root.addView(action("2", "Overlay permission", "Required for the landscape controls.", v->openOverlayPermission()));
        root.addView(action("3", "Start input", "Enable the landscape mouse + controls.", v->startBridge()));
        root.addView(action("4", "Stop input", "Release all held keys/buttons and stop.", v->stopBridge()));

        TextView controls=text("CONTROLS",12,MUTED);
        controls.setTypeface(Typeface.DEFAULT,Typeface.BOLD);
        root.addView(controls,lp(1,dp(34)));

        Button editor=primary("Open Custom Controls");
        editor.setOnClickListener(v->startActivity(new Intent(this,ControlEditorActivity.class)));
        root.addView(editor,lp(1,dp(58)));

        LinearLayout tip=card();
        TextView tipTitle=text("Landscape mode",15,TEXT);
        tipTitle.setTypeface(Typeface.DEFAULT,Typeface.BOLD);
        tip.addView(tipTitle);
        tip.addView(text("The overlay is completely disabled in portrait. In landscape, use HIDE to collapse it to a small SHOW handle. The right side is a true relative mouse surface with a visible software cursor.",13,MUTED),lp(1,dp(78)));
        root.addView(tip,lp(1,dp(135)));

        setContentView(scroll);
    }

    private View action(String n,String title,String sub,View.OnClickListener listener){
        LinearLayout row=card();
        row.setOrientation(LinearLayout.HORIZONTAL);
        TextView num=text(n,13,TEXT);
        num.setGravity(Gravity.CENTER);
        num.setTypeface(Typeface.DEFAULT,Typeface.BOLD);
        num.setBackground(round(ACCENT,dp(18)));
        row.addView(num,badgeLp());
        LinearLayout words=new LinearLayout(this);
        words.setOrientation(LinearLayout.VERTICAL);
        words.setPadding(dp(14),0,0,0);
        TextView t=text(title,15,TEXT);t.setTypeface(Typeface.DEFAULT,Typeface.BOLD);
        TextView s=text(sub,12,MUTED);
        words.addView(t);words.addView(s,lp(1,dp(32)));
        row.addView(words,lp(1,dp(62)));
        row.setOnClickListener(listener);
        return row;
    }

    private void openShizuku(){
        try{startActivity(new Intent("moe.shizuku.manager.intent.action.MANAGER"));}
        catch(Exception e){toast("Open Shizuku manually.");}
    }

    private void openOverlayPermission(){
        try{startActivity(new Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION,Uri.parse("package:"+getPackageName())));}
        catch(Exception e){toast("Open Android overlay permission manually.");}
    }

    private void startBridge(){
        if(!bridge.isShizukuRunning()){toast("Shizuku is not running.");return;}
        if(!bridge.hasPermission()){bridge.requestPermission();toast("Grant Shizuku permission, then tap Start again.");return;}
        if(!Settings.canDrawOverlays(this)){toast("Allow overlay permission first.");return;}
        if(!bridge.startService()){toast("Could not bind the Shizuku injector.");return;}
        Intent i=new Intent(this,OverlayService.class);
        if(android.os.Build.VERSION.SDK_INT>=26)startForegroundService(i);else startService(i);
        toast("Input bridge started.");
        refresh();
    }

    private void stopBridge(){
        stopService(new Intent(this,OverlayService.class));
        bridge.releaseAll();
        bridge.stopService();
        toast("Input bridge stopped.");
        refresh();
    }

    private void refresh(){
        boolean s=bridge.isShizukuRunning(),p=bridge.hasPermission(),o=Settings.canDrawOverlays(this);
        String text="Shizuku       "+(s?"READY":"NOT RUNNING")+
                "\nPermission   "+(p?"GRANTED":"NOT GRANTED")+
                "\nOverlay       "+(o?"GRANTED":"NOT GRANTED")+
                "\nInjector      "+bridge.status();
        status.setText(text);
        boolean ready=s&&p&&o;
        badge.setText(ready?"READY":"SETUP");
        badge.setBackground(round(ready?Color.rgb(39,160,104):Color.rgb(110,82,20),dp(18)));
    }

    private Button primary(String s){
        Button b=new Button(this);b.setText(s);b.setTextColor(TEXT);b.setTextSize(14);b.setAllCaps(false);
        b.setTypeface(Typeface.DEFAULT,Typeface.BOLD);b.setBackground(round(ACCENT,dp(16)));return b;
    }

    private LinearLayout card(){
        LinearLayout l=new LinearLayout(this);
        l.setOrientation(LinearLayout.VERTICAL);
        l.setPadding(dp(16),dp(14),dp(16),dp(14));
        l.setBackground(round(CARD,dp(20)));
        return l;
    }

    private TextView text(String s,float size,int color){
        TextView t=new TextView(this);t.setText(s);t.setTextSize(size);t.setTextColor(color);return t;
    }

    private GradientDrawable round(int color,int radius){
        GradientDrawable g=new GradientDrawable();g.setColor(color);g.setCornerRadius(radius);return g;
    }

    private LinearLayout.LayoutParams lp(float weight,int height){
        return new LinearLayout.LayoutParams(-1,height,weight==1?0:weight);
    }

    private LinearLayout.LayoutParams badgeLp(){
        LinearLayout.LayoutParams p=new LinearLayout.LayoutParams(dp(56),dp(36));
        p.gravity=Gravity.CENTER_VERTICAL;return p;
    }

    private int dp(int n){return Math.round(n*getResources().getDisplayMetrics().density);}
    private void toast(String s){Toast.makeText(this,s,Toast.LENGTH_SHORT).show();}
}
