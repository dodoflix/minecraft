# Screenshot comparison

`scripts/client_e2e/shotdiff.py A.png B.png` compares two screenshots of the real client region by region, with the
standard library only (8-bit PNGs, every scanline filter).

```sh
scripts/client_e2e/shotdiff.py vanilla/shots/hud.png ours/shots/hud.png --diff run/hud-diff.png
scripts/client_e2e/shotdiff.py a.png b.png --crop 0,0,427,240 --grid 2 --mean-threshold 1.5
scripts/client_e2e/shotdiff.py a.png b.png --region 0,0,100,50,0.5,0.001 --region 100,0,200,50
```

* `--crop X,Y,W,H` cuts both images first; `--grid N` (default 4) splits the result into N x N regions, `--region
  X,Y,W,H[,MEAN[,RATIO]]` (repeatable, relative to the crop) replaces the grid with explicit regions that may carry
  their own thresholds.
* Per region: the mean absolute channel difference (0..255) and the ratio of changed pixels (a pixel changed when any
  channel differs by more than `--pixel-tolerance`, default 8). A region fails above `--mean-threshold` (default 2.0) or
  `--ratio-threshold` (default 0.02).
* `--diff OUT.png` writes the changed pixels in red, scaled by the difference; the rest is dimmed.
* Output is one JSON object; exit 0 iff every region passes, 1 on a difference or on images of different sizes, 2 on
  unreadable input or a bad option.

Screenshots only mean something when both were taken on the same machine in the same run: GPU and driver change how a
frame is rendered. Use frozen scenes (`/tick freeze`, weather clear, fixed time, the deterministic option profile of
`scripts/real-client.sh`) and compare the two servers' frames with each other, never with a committed image. The default
thresholds come from generated frames; they are not tuned against a GPU yet, so a scenario that needs looser limits
says so with `--region` thresholds rather than a global change.
