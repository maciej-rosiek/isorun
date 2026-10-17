import { defineConfig } from 'vitest/config'
export default defineConfig({
  test: {
    // the hosted page's Worker; the Rust app's tests are `cargo test --workspace` in native/
    include: ['worker/test/**/*.test.ts'],
  },
})
