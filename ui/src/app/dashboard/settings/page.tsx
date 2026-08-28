import { requireSession } from "@/lib/session";
import { Metadata } from 'next';
import BackLink from "@/components/BackLink";
import GlassCard from "@/components/GlassCard";
import GradientHeading from "@/components/GradientHeading";
import SettingsForm from "./SettingsForm";

export const metadata: Metadata = {
  title: 'Nano-Bank - Profile Settings',
};

export default async function SettingsPage() {
    const { profile } = await requireSession();

    return (
        <main className="relative z-10 flex-1 flex flex-col items-center justify-center px-6 py-12">
            <div className="w-full max-w-3xl">
                <BackLink href="/dashboard">Back to Dashboard</BackLink>

                {/* Content Card */}
                <GlassCard>
                    <div className="mb-8 border-b border-white/10 pb-6">
                        <GradientHeading>Profile Settings</GradientHeading>
                        <p className="text-slate-400 text-sm mt-2">
                          Manage your contact details, login information, and security preferences.
                        </p>
                    </div>

                    <SettingsForm profile={profile} />
                </GlassCard>
            </div>
        </main>
    );
}
