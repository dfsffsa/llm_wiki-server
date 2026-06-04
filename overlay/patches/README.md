# Upstream patches

仅在无法通过 overlay 扩展解决时，才在此存放对 `upstream/` 的最小 patch。

应用方式（待 `scripts/apply-patches.sh` 实现）：

```bash
cd upstream
git apply ../overlay/patches/0001-example.patch
```

**当前策略：Phase 0–1 零 patch**，headless 逻辑全部在 `overlay/server/`。
