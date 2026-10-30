import { HashRouter, Navigate, Outlet, Route, Routes, useParams } from "react-router-dom";
import { Sidebar } from "@/components/shell/sidebar";
import { Toaster } from "@/components/ui/sonner";
import { DEFAULT_SCOPE, isScope, type Scope } from "@/lib/scope";
import CheckinPage from "@/pages/CheckinPage";
import CreditsPage from "@/pages/CreditsPage";
import SettingsPage from "@/pages/SettingsPage";

function ScopeLayout() {
  const params = useParams();
  const raw = params.scope ?? "";
  if (!isScope(raw)) return <Navigate to={`/${DEFAULT_SCOPE}/checkin`} replace />;
  const scope: Scope = raw;

  return (
    <div className="flex h-screen min-h-0 overflow-hidden bg-background">
      <Sidebar scope={scope} />
      <main className="min-w-0 flex-1 overflow-y-auto bg-background overscroll-contain">
        <Outlet context={scope} />
      </main>
    </div>
  );
}

export default function App() {
  return (
    <HashRouter>
      <Routes>
        <Route path="/" element={<Navigate to={`/${DEFAULT_SCOPE}/checkin`} replace />} />
        <Route path="/:scope" element={<ScopeLayout />}>
          <Route index element={<Navigate to="checkin" replace />} />
          <Route path="checkin" element={<CheckinPage />} />
          <Route path="credits" element={<CreditsPage />} />
          <Route path="settings" element={<SettingsPage />} />
        </Route>
        <Route path="*" element={<Navigate to={`/${DEFAULT_SCOPE}/checkin`} replace />} />
      </Routes>
      <Toaster />
    </HashRouter>
  );
}
