import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(() => {
  cleanup();
});

// jsdom has no layout: cmdk (the command palette) observes its list's size
// and scrolls the selected item into view. Neither matters in a test.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
Element.prototype.scrollIntoView ??= function scrollIntoView() {};

// Uploads in tests: Vitest's jsdom bridge turns a jsdom FormData into Node's
// FormData, with each jsdom File as a Node Blob. From Node 26, Node's FormData
// wraps a Blob (when sending) and each parsed part (in `request.formData()`)
// with whatever the global `File` is, here jsdom's, which cannot hold Node's
// data: it keeps the text "[object Blob]" or fails an internal assertion.
// So Node's own File takes the global slot while a Request is built or its
// form is read. Browsers and the desktop app never mix the two.

/** Node's `File`, without adding Node's types to the app's type check. */
interface NodeProcess {
  getBuiltinModule(id: "node:buffer"): { File: typeof File };
}
const nodeProcess = (globalThis as unknown as { process: NodeProcess }).process;
const NodeFile = nodeProcess.getBuiltinModule("node:buffer").File;
const JsdomFile = globalThis.File;

async function withNodeFile<T>(work: () => Promise<T>): Promise<T> {
  globalThis.File = NodeFile;
  try {
    return await work();
  } finally {
    globalThis.File = JsdomFile;
  }
}

// Sending: wrap Vitest's bridge Request, which converts the form.
const BridgeRequest = globalThis.Request;
globalThis.Request = class RequestWithNodeFile extends BridgeRequest {
  constructor(...args: ConstructorParameters<typeof Request>) {
    globalThis.File = NodeFile;
    try {
      super(...args);
    } finally {
      globalThis.File = JsdomFile;
    }
  }
};

// Reading: on Node's own Request, because the fake API keeps `request.clone()`,
// which is a plain Node Request. Only tests read forms, one test at a time,
// so the swap across the await does not leak into other code.
const nodeRequestProto = Object.getPrototypeOf(BridgeRequest.prototype) as Request;
const nodeFormData = nodeRequestProto.formData;
nodeRequestProto.formData = function formData(this: Request) {
  return withNodeFile(() => nodeFormData.call(this));
};
