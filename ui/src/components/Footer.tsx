import Link from "next/link";
import SystemStatus from "./SystemStatus";

interface FooterLink {
  label: string;
  href: string;
}

interface FooterProps {
  links?: FooterLink[];
}

const DEFAULT_LINKS: FooterLink[] = [
  { label: "Privacy Policy", href: "/privacy" },
  { label: "Terms of Service", href: "/terms" },
];

export default function Footer({ links = DEFAULT_LINKS }: FooterProps) {
  // Filter out any "Health Check" links for backward compatibility
  const filteredLinks = links.filter(
    (link) => link.href !== "/health" && link.label.toLowerCase() !== "health check"
  );

  return (
    <footer className="relative z-10 w-full max-w-7xl mx-auto px-6 py-6 flex flex-col sm:flex-row items-center justify-between border-t border-white/5 gap-4">
      <p className="text-xs text-slate-500">&copy; {new Date().getFullYear()} Nano-Bank. All rights reserved.</p>
      <div className="flex items-center gap-6 text-xs text-slate-500">
        <SystemStatus />
        {filteredLinks.map((link) => (
          <Link key={link.label} href={link.href} className="hover:text-slate-300 transition-colors">
            {link.label}
          </Link>
        ))}
      </div>
    </footer>
  );
}
