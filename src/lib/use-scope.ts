import { useOutletContext } from "react-router-dom";
import type { Scope } from "./scope";

export function useScope(): Scope {
  return useOutletContext<Scope>();
}
