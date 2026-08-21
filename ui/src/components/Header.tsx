import Link from "next/link";
import { cookies } from "next/headers";
import { verifySession, type CustomerProfile } from "@/lib/session";
import ProfileDropdown from "./ProfileDropdown";

export default async function Header() {
    const cookieStore = await cookies();
    // The refresh_token is the durable session marker; the access_token is
    // short-lived and can be absent between refreshes while the session is live.
    const isAuthenticated = Boolean(cookieStore.get("refresh_token")?.value);

    let profile: CustomerProfile | null = null;
    if (isAuthenticated) {
        const accessToken = cookieStore.get("access_token")?.value;
        const verification = await verifySession(accessToken);
        if (verification.status === "valid") {
            profile = verification.profile;
        }

        // Intercept and merge updated profile cookie if present
        const updatedProfileCookie = cookieStore.get("updated_profile")?.value;
        if (updatedProfileCookie) {
            try {
                const overrides = JSON.parse(updatedProfileCookie);
                profile = profile ? { ...profile, ...overrides } : overrides;
            } catch (e) {
                console.error("Failed to parse updated_profile cookie:", e);
            }
        }
    }

    return (
        <header className="relative z-50 w-full max-w-7xl mx-auto px-6 py-6 flex items-center justify-between">
            <Link href="/" className="flex items-center gap-2 group">
                <div className="w-8 h-8 rounded-lg bg-gradient-to-tr from-nanobank-blue-green to-nanobank-blue-sky flex items-center justify-center font-bold text-nanobank-blue-deep shadow-md transform group-hover:scale-105 transition-transform">
                    N
                </div>
                <span className="text-xl font-bold tracking-tight bg-gradient-to-r from-white via-slate-100 to-nanobank-blue-sky bg-clip-text text-transparent">
                    Nano-Bank
                </span>
            </Link>

            {isAuthenticated ? (
                <div className="flex items-center gap-6">
                    <Link
                        href="/dashboard"
                        className="text-sm font-medium text-nanobank-blue-sky hover:text-white transition-colors duration-200"
                    >
                        Dashboard
                    </Link>
                    <ProfileDropdown profile={profile} />
                </div>
            ) : (
                <Link
                    href="/auth/signin"
                    className="text-sm font-medium text-nanobank-blue-sky hover:text-white transition-colors duration-200"
                >
                    Sign In
                </Link>
            )}
        </header>
    );
}
