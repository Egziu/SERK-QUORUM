import { Navigate, Route, Routes, useLocation } from "react-router";
import { Layout } from "./components/Layout";
import { Loading } from "./components/ui";
import { AcceptInvitationPage, AuditPage, MembersPage, NewOrganizationPage, ProvidersPage } from "./pages/Admin";
import { LoginPage, SignupPage } from "./pages/Auth";
import { CommandPage } from "./pages/Command";
import { DecisionPage } from "./pages/DecisionPage";
import { DocumentPage } from "./pages/DocumentPage";
import { HomePage } from "./pages/Home";
import { ProjectDetailPage } from "./pages/ProjectDetail";
import { ProjectsPage } from "./pages/Projects";
import { useSession } from "./session";

function RequireSession({ children }: { children: React.ReactNode }) {
  const { status } = useSession();
  const location = useLocation();
  if (status === "loading") {
    return (
      <div className="auth">
        <Loading />
      </div>
    );
  }
  if (status === "anonymous") {
    const next = encodeURIComponent(location.pathname + location.search + location.hash);
    return <Navigate to={`/login?next=${next}`} replace />;
  }
  return <>{children}</>;
}

function Landing() {
  const { session } = useSession();
  const first = session?.memberships[0];
  return first ? <Navigate to={`/o/${first.organization_id}`} replace /> : <Navigate to="/organizations/new" replace />;
}

export function App() {
  return (
    <Routes>
      <Route path="/login" element={<LoginPage />} />
      <Route path="/signup" element={<SignupPage />} />
      <Route
        path="/invitations/accept"
        element={
          <RequireSession>
            <AcceptInvitationPage />
          </RequireSession>
        }
      />
      <Route
        path="/organizations/new"
        element={
          <RequireSession>
            <NewOrganizationPage />
          </RequireSession>
        }
      />
      <Route
        path="/o/:org"
        element={
          <RequireSession>
            <Layout />
          </RequireSession>
        }
      >
        <Route index element={<HomePage />} />
        <Route path="command" element={<CommandPage />} />
        <Route path="projects" element={<ProjectsPage />} />
        <Route path="projects/:project" element={<ProjectDetailPage />} />
        <Route path="documents/:document" element={<DocumentPage />} />
        <Route path="decisions/:decision" element={<DecisionPage />} />
        <Route path="members" element={<MembersPage />} />
        <Route path="providers" element={<ProvidersPage />} />
        <Route path="audit" element={<AuditPage />} />
      </Route>
      <Route
        path="*"
        element={
          <RequireSession>
            <Landing />
          </RequireSession>
        }
      />
    </Routes>
  );
}
