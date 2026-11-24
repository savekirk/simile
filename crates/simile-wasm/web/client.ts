import type { Methods, Request, Response } from "./contract.js";

/** Calls are serialized so a reset/start reply updates the generation before the next call. */
export class SimileClient {
  private generation = "0";
  private nextId = 0n;
  private queue: Promise<unknown> = Promise.resolve();
  private readonly worker: Worker;
  constructor(worker: Worker) { this.worker = worker; }

  call<K extends keyof Methods>(method: K, input: Methods[K]["input"]): Promise<Methods[K]["output"]> {
    const next = this.queue.then(() => new Promise<Methods[K]["output"]>((resolve, reject) => {
      const id = (++this.nextId).toString();
      const receive = (event: MessageEvent<Response>) => {
        const response = event.data;
        if (response.id !== id) return;
        this.worker.removeEventListener("message", receive);
        this.worker.removeEventListener("error", failed);
        this.generation = response.generation;
        if (response.ok) resolve(response.value as Methods[K]["output"]);
        else reject(response.error);
      };
      const failed = (event: ErrorEvent) => {
        this.worker.removeEventListener("message", receive);
        this.worker.removeEventListener("error", failed);
        reject({ code: "worker_error", message: event.message });
      };
      this.worker.addEventListener("message", receive);
      this.worker.addEventListener("error", failed);
      // Inputs are cloned. Callers retain artifact and checkpoint buffers for reuse.
      this.worker.postMessage({ id, generation: this.generation, method, input } as Request);
    }));
    this.queue = next.catch(() => undefined);
    return next;
  }
}
