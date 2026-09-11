import Link from "next/link";
import { type GuidanceItem } from "@/lib/guidance";

interface GuidanceCalloutProps extends GuidanceItem {
  variant?: "info" | "warning" | "success";
}

export function GuidanceCallout({
  variant = "info",
  title,
  message,
  linkPath,
  linkLabel,
}: GuidanceCalloutProps) {
  const variantStyles = {
    info: "bg-blue-50 border-blue-200 text-blue-900",
    warning: "bg-amber-50 border-amber-200 text-amber-900",
    success: "bg-green-50 border-green-200 text-green-900",
  };

  return (
    <div className={`border rounded-lg p-4 ${variantStyles[variant]}`}>
      {title && <h4 className="font-semibold text-sm mb-2">{title}</h4>}
      <p className="text-sm mb-2">{message}</p>
      {linkPath && linkLabel && (
        <Link href={linkPath} className="text-sm font-semibold underline">
          {linkLabel}
        </Link>
      )}
    </div>
  );
}
