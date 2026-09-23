import { UselessButtonElement } from "./element.js";

export { UselessButtonElement };
export type { UselessButton } from "./wasm.js";

const DEFAULT_TAG = "useless-button";

/**
 * Register the element under a given tag name. Called automatically for
 * `"useless-button"` as soon as this module is imported; call it again
 * yourself if you also want the component available under another tag.
 *
 * The Custom Elements registry rejects registering the *same* class
 * under a second name, so any tag beyond the default gets a trivial,
 * behaviorally-identical subclass instead.
 */
export function defineUselessButton(tag: string = DEFAULT_TAG): void {
  if (typeof customElements === "undefined") return;
  if (customElements.get(tag)) return;
  const ctor = tag === DEFAULT_TAG ? UselessButtonElement : class extends UselessButtonElement {};
  customElements.define(tag, ctor);
}

defineUselessButton();
