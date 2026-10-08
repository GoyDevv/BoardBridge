package com.goydevv.inputbridge;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import android.os.Build;
import android.os.IBinder;
import android.os.SystemClock;
import android.view.MotionEvent;
import android.view.View;
import android.view.WindowManager;

import java.util.HashMap;
import java.util.List;
import java.util.Map;

public final class OverlayService extends Service {
    private WindowManager wm;
    private OverlayView view;

    @Override public void onCreate() {
        super.onCreate();
        startForegroundCompat();
        wm=(WindowManager)getSystemService(WINDOW_SERVICE);
        view=new OverlayView();
        WindowManager.LayoutParams p=new WindowManager.LayoutParams(
                WindowManager.LayoutParams.MATCH_PARENT,
                WindowManager.LayoutParams.MATCH_PARENT,
                Build.VERSION.SDK_INT>=26 ? WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY : WindowManager.LayoutParams.TYPE_PHONE,
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE | WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
                android.graphics.PixelFormat.TRANSLUCENT);
        wm.addView(view,p);
    }

    @Override public int onStartCommand(Intent intent,int flags,int id){return START_STICKY;}

    @Override public void onDestroy(){
        if(view!=null){view.releaseAll();try{wm.removeView(view);}catch(Throwable ignored){}view=null;}
        super.onDestroy();
    }

    @Override public IBinder onBind(Intent intent){return null;}

    private void startForegroundCompat(){
        String id="xcloud-input";
        NotificationManager nm=(NotificationManager)getSystemService(NOTIFICATION_SERVICE);
        if(Build.VERSION.SDK_INT>=26)nm.createNotificationChannel(new NotificationChannel(id,"Xcloud Input",NotificationManager.IMPORTANCE_LOW));
        Intent open=new Intent(this,MainActivity.class);
        PendingIntent pi=PendingIntent.getActivity(this,0,open,PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_UPDATE_CURRENT);
        Notification n=new Notification.Builder(this,id)
                .setSmallIcon(android.R.drawable.ic_media_play)
                .setContentTitle("Xcloud Input Bridge")
                .setContentText("Relative input overlay active")
                .setContentIntent(pi).setOngoing(true).build();
        if(Build.VERSION.SDK_INT>=29)startForeground(9,n,android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE);
        else startForeground(9,n);
    }

    private final class OverlayView extends View {
        private final Paint paint=new Paint(Paint.ANTI_ALIAS_FLAG);
        private final BridgeClient bridge=BridgeClient.get(OverlayService.this);
        private final List<ControlModel.Control> controls=ControlModel.load(OverlayService.this);
        private final Map<Integer,ControlModel.Control> active=new HashMap<>();
        private float lastMouseX,lastMouseY;
        private boolean mousePadActive;

        OverlayView(){super(OverlayService.this);setBackgroundColor(Color.TRANSPARENT);setLayerType(View.LAYER_TYPE_SOFTWARE,null);}

        @Override protected void onDraw(Canvas c){
            super.onDraw(c);
            int w=getWidth(),h=getHeight();
            paint.setStyle(Paint.Style.FILL);
            paint.setColor(Color.argb(35,255,255,255));
            c.drawRect(w*0.48f,0,w,h*0.62f,paint);
            paint.setColor(Color.argb(65,255,255,255));
            c.drawText("AIM / MOUSE",w*0.52f,h*0.08f,paint);
            for(ControlModel.Control x:controls){
                if(!x.visible)continue;
                float px=x.x*w,py=x.y*h,rw=x.w*w,rh=x.h*h;
                paint.setColor(Color.argb((int)(255*x.opacity),80,85,100));
                if(x.type==ControlModel.JOYSTICK||x.type==ControlModel.DPAD){
                    c.drawCircle(px,py,Math.min(rw,rh)*.5f,paint);
                }else{
                    c.drawRoundRect(new RectF(px-rw*.5f,py-rh*.5f,px+rw*.5f,py+rh*.5f),16,16,paint);
                }
                paint.setColor(Color.WHITE);paint.setTextSize(Math.max(12,rh*.22f));paint.setTextAlign(Paint.Align.CENTER);
                c.drawText(x.label,px,py+paint.getTextSize()*.35f,paint);
            }
        }

        @Override public boolean onTouchEvent(MotionEvent e){
            int action=e.getActionMasked(),idx=e.getActionIndex(),pid=e.getPointerId(idx);
            float x=e.getX(idx),y=e.getY(idx);
            if(action==MotionEvent.ACTION_DOWN){
                if(isMousePad(x,y)){mousePadActive=true;lastMouseX=x;lastMouseY=y;return true;}
                ControlModel.Control c=find(x/getWidth(),y/getHeight());
                if(c!=null){active.put(pid,c);apply(c,true,x,y);return true;}
                return true;
            }
            if(action==MotionEvent.ACTION_POINTER_DOWN){
                if(isMousePad(x,y)){mousePadActive=true;lastMouseX=x;lastMouseY=y;return true;}
                ControlModel.Control c=find(x/getWidth(),y/getHeight());
                if(c!=null){active.put(pid,c);apply(c,true,x,y);}
                return true;
            }
            if(action==MotionEvent.ACTION_MOVE){
                if(mousePadActive){
                    int n=e.getPointerCount();
                    for(int i=0;i<n;i++){
                        int p=e.getPointerId(i);
                        float nx=e.getX(i),ny=e.getY(i);
                        if(p==pid || i==0){
                            float dx=nx-lastMouseX,dy=ny-lastMouseY;
                            lastMouseX=nx;lastMouseY=ny;
                            if(dx!=0f||dy!=0f)bridge.mouseMove(dx,dy,SystemClock.uptimeMillis());
                            break;
                        }
                    }
                }
                for(int i=0;i<e.getPointerCount();i++){
                    ControlModel.Control c=active.get(e.getPointerId(i));
                    if(c!=null)updateControl(c,e.getX(i),e.getY(i));
                }
                return true;
            }
            if(action==MotionEvent.ACTION_UP||action==MotionEvent.ACTION_POINTER_UP||action==MotionEvent.ACTION_CANCEL){
                ControlModel.Control c=active.remove(pid);
                if(c!=null)releaseControl(c);
                if(action==MotionEvent.ACTION_UP)mousePadActive=false;
                if(action==MotionEvent.ACTION_CANCEL)releaseAll();
                return true;
            }
            return true;
        }

        private boolean isMousePad(float x,float y){return x>getWidth()*.48f && y<getHeight()*.62f;}
        private ControlModel.Control find(float x,float y){
            for(int i=controls.size()-1;i>=0;i--){
                ControlModel.Control c=controls.get(i);
                if(!c.visible||c.type==ControlModel.DRAWER)continue;
                if(Math.abs(x-c.x)<=c.w*.5f&&Math.abs(y-c.y)<=c.h*.5f)return c;
            }
            return null;
        }

        private void apply(ControlModel.Control c,boolean down,float x,float y){
            if(c.type==ControlModel.JOYSTICK||c.type==ControlModel.DPAD){updateControl(c,x,y);return;}
            if(c.actions.isEmpty())return;
            for(ControlModel.Action a:c.actions)emit(a,down);
        }

        private void updateControl(ControlModel.Control c,float x,float y){
            if(c.actions.size()<4)return;
            float nx=(x/getWidth()-c.x)/(c.w*.5f),ny=(y/getHeight()-c.y)/(c.h*.5f);
            int left,right,up,down;
            if(c.type==ControlModel.JOYSTICK){left=c.actions.get(0).code;right=c.actions.get(1).code;up=c.actions.get(2).code;down=c.actions.get(3).code;}
            else{left=c.actions.get(0).code;right=c.actions.get(1).code;up=c.actions.get(2).code;down=c.actions.get(3).code;}
            releaseAxis(left,right,up,down,nx,ny);
        }

        private void releaseAxis(int l,int r,int u,int d,float x,float y){
            boolean L=x<-.25f,R=x>.25f,U=y<-.25f,D=y>.25f;
            bridge.keyEvent(l,L,0,SystemClock.uptimeMillis());
            bridge.keyEvent(r,R,0,SystemClock.uptimeMillis());
            bridge.keyEvent(u,U,0,SystemClock.uptimeMillis());
            bridge.keyEvent(d,D,0,SystemClock.uptimeMillis());
        }

        private void releaseControl(ControlModel.Control c){
            if(c.type==ControlModel.JOYSTICK||c.type==ControlModel.DPAD){
                if(c.actions.size()>=4){for(int i=0;i<4;i++)bridge.keyEvent(c.actions.get(i).code,false,0,SystemClock.uptimeMillis());}
            }else{
                for(ControlModel.Action a:c.actions)emit(a,false);
            }
        }

        private void emit(ControlModel.Action a,boolean down){
            long t=SystemClock.uptimeMillis();
            if(a.type==ControlModel.KEY)bridge.keyEvent(a.code,down,0,t);
            else if(a.type==ControlModel.MOUSE_BUTTON)bridge.mouseButton(a.code,down,t);
            else if(a.type==ControlModel.SCROLL&&down)bridge.mouseScroll(0,a.value,t);
        }

        void releaseAll(){bridge.releaseAll();active.clear();mousePadActive=false;}
    }
}
