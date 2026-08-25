import { brotliCompressSync, constants, gzipSync } from "node:zlib";
import type { Plugin } from "vite";

/**
 * Extensions worth compressing. Everything else in a bundle is already
 * compressed (png, woff2, wasm sections), and running deflate over those costs
 * bytes in the binary and gains nothing on the wire.
 */
const COMPRESSIBLE = /\.(js|mjs|css|html|svg|json|map|txt|ico)$/;

/**
 * Below this, the header overhead of a second representation is most of the
 * transfer and the saving is noise.
 */
const MINIMUM_BYTES = 256;

/**
 * Writes a `.br` and a `.gz` beside every compressible file in the bundle.
 *
 * The node embeds `ui/dist` with rust-embed and serves whichever
 * representation the request's `Accept-Encoding` allows, so the compression
 * runs once at build time rather than once per request. It costs binary size
 * and saves the node the CPU of deflating the same 386 kB bundle for every
 * visitor.
 *
 * A sibling is written only when it is actually smaller: a file that does not
 * compress keeps one representation and is served as it is.
 */
export function precompress(): Plugin {
  return {
    name: "mabel-precompress",
    apply: "build",
    enforce: "post",
    generateBundle(_options, bundle) {
      for (const [fileName, output] of Object.entries(bundle)) {
        if (!COMPRESSIBLE.test(fileName)) {
          continue;
        }
        const source = output.type === "asset" ? output.source : output.code;
        const bytes = typeof source === "string" ? Buffer.from(source) : Buffer.from(source);
        if (bytes.byteLength < MINIMUM_BYTES) {
          continue;
        }
        const brotli = brotliCompressSync(bytes, {
          params: {
            [constants.BROTLI_PARAM_QUALITY]: constants.BROTLI_MAX_QUALITY,
            [constants.BROTLI_PARAM_SIZE_HINT]: bytes.byteLength,
          },
        });
        const gzip = gzipSync(bytes, { level: 9 });
        if (brotli.byteLength < bytes.byteLength) {
          this.emitFile({ type: "asset", fileName: `${fileName}.br`, source: brotli });
        }
        if (gzip.byteLength < bytes.byteLength) {
          this.emitFile({ type: "asset", fileName: `${fileName}.gz`, source: gzip });
        }
      }
    },
  };
}
