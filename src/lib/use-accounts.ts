import { useCallback, useEffect, useState } from "react";
import * as api from "./api";
import type { AccountMeta } from "./types";

export function useAccounts(scope: string) {
  const [accounts, setAccounts] = useState<AccountMeta[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setError(null);
      const list = await api.listAccounts(scope);
      setAccounts(list);
    } catch (cause) {
      setError(api.asError(cause));
      setAccounts([]);
    } finally {
      setLoading(false);
    }
  }, [scope]);

  useEffect(() => {
    setLoading(true);
    void refresh();
  }, [refresh]);

  return { accounts, loading, error, refresh };
}
