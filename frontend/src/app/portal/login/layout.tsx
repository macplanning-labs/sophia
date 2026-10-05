export default function PortalLoginLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  // ポータルログインはSidebarなし
  return <>{children}</>;
}
