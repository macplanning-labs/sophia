export default function LoginLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  // ログインページはSidebarなし
  return <>{children}</>;
}
