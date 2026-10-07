package com.xtg.pad;

interface IVPad {
  String start() = 1;
  void report(in byte[] r) = 2;
  void stop() = 3;
  String diag() = 4;
  void destroy() = 16777114;
}
