import { requireSession } from "@/lib/session";
import { Metadata } from 'next';
import BackLink from "@/components/BackLink";
import GlassCard from "@/components/GlassCard";
import GradientHeading from "@/components/GradientHeading";

export const metadata: Metadata = {
  title: 'Nano-Bank - Open New Account',
};

export default async function CreateAccountPage() {
    await requireSession();

    return (
        <main className="relative z-10 flex-1 flex flex-col items-center justify-center px-6 py-12">
            <div className="w-full max-w-3xl">
                <BackLink href="/dashboard/accounts">Back to Accounts</BackLink>

                {/* Content Card */}
                <GlassCard>
                    <div className="mb-8 border-b border-white/10 pb-6">
                        <GradientHeading>Open a New Account</GradientHeading>
                    </div>

                    <div className="rounded-xl border border-dashed border-slate-700 bg-slate-900/30 p-8 text-center text-sm text-slate-400">
                        Account creation form will come here.
                    </div>
                </GlassCard>
            </div>
        </main>
    );
}
