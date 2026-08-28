import { requireSession } from "@/lib/session";
import { Metadata } from 'next';
import { AlertCircle } from "lucide-react";
import { API_BASE_URL } from "@/lib/config";
import { Account } from "@/lib/accounts";
import BackLink from "@/components/BackLink";
import GlassCard from "@/components/GlassCard";
import GradientHeading from "@/components/GradientHeading";
import DepositForm from "./DepositForm";

export const metadata: Metadata = {
  title: 'Nano-Bank - Deposit Money',
};

type Props = {
  searchParams: Promise<{ account?: string }>;
};

export default async function DepositPage({ searchParams }: Props) {
    const { accessToken } = await requireSession();
    const { account } = await searchParams;

    let accounts: Account[] = [];
    let fetchError = false;
    try {
        const response = await fetch(`${API_BASE_URL}/api/v1/accounts`, {
            headers: { Authorization: `Bearer ${accessToken}` },
            cache: "no-store",
        });
        if (response.ok) {
            accounts = await response.json();
        } else {
            console.error(`Failed to fetch accounts: ${response.status}`);
            fetchError = true;
        }
    } catch (error) {
        console.error("Failed to fetch accounts:", error);
        fetchError = true;
    }

    // Deposits only land in chequing/savings accounts; credit cards settle
    // through the card rails, not this flow.
    const depositableAccounts = accounts.filter(
        (a) => (a.account_type === "chequing" || a.account_type === "savings") && a.status === "active"
    );

    return (
        <main className="relative z-10 flex-1 flex flex-col items-center justify-center px-6 py-12">
            <div className="w-full max-w-3xl">
                <BackLink href="/dashboard/accounts">Back to Accounts</BackLink>

                <GlassCard>
                    <div className="mb-8 border-b border-white/10 pb-6">
                        <GradientHeading>Deposit Money</GradientHeading>
                        <p className="text-slate-400 text-sm mt-2">
                            Add money to one of your chequing or savings accounts.
                        </p>
                    </div>

                    {fetchError ? (
                        <div className="flex items-center gap-3 p-4 rounded-xl border border-rose-500/20 bg-rose-500/10 text-rose-300 text-sm">
                            <AlertCircle className="w-5 h-5 flex-shrink-0" />
                            <div>
                                <span className="font-semibold">Error fetching your accounts</span>
                            </div>
                        </div>
                    ) : depositableAccounts.length === 0 ? (
                        <div className="rounded-xl border border-dashed border-slate-700 bg-slate-900/30 p-8 text-center text-sm text-slate-400">
                            You need an active chequing or savings account to make a deposit.
                        </div>
                    ) : (
                        <DepositForm accounts={depositableAccounts} initialAccountId={account} />
                    )}
                </GlassCard>
            </div>
        </main>
    );
}
