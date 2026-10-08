package com.goydevv.inputbridge;

interface IInputBridge {
    void destroy() = 16777114;

    oneway void mouseMove(float dx, float dy, long eventTime) = 1;
    oneway void mouseButton(int button, boolean pressed, long eventTime) = 2;
    oneway void mouseScroll(float horizontal, float vertical, long eventTime) = 3;
    oneway void keyEvent(int keyCode, boolean pressed, int metaState, long eventTime) = 4;
    oneway void releaseAll() = 5;

    String getStatus();
    int getUid();
}
