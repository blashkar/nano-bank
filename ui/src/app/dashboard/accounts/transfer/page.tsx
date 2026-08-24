import { requireSession } from "@/lib/session";
import { Metadata } from 'next';
import { AlertCircle } from "lucide-react";
import { API_BASE_URL } from "@/lib/config";
import { Account } from "@/lib/accounts";
import BackLink from "@/components/BackLink";
import GlassCard from "@/components/GlassCard";
import GradientHeading from "@/components/GradientHeading";
import TransferForm from "./TransferForm";

export const metadata: Metadata = {
  title: 'Nano-Bank - Transfer Money',
};

type Props = {
  searchParams: Promise<{ from?: string }>;
};

export default async function TransferPage({ searchParams }: Props) {
    const { accessToken } = await requireSession();
    const { from } = await searchParams;

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

    // Only chequing/savings accounts can send or receive an internal transfer;
    // credit cards settle through the card rails, not this flow.
    const transferableAccounts = accounts.filter(
        (a) => (a.account_type === "chequing" || a.account_type === "savings") && a.status === "active"
    );

    return (
        <main className="relative z-10 flex-1 flex flex-col items-center justify-center px-6 py-12">
            <div className="w-full max-w-3xl">
                <BackLink href="/dashboard/accounts">Back to Accounts</BackLink>

                <GlassCard>
                    <div className="mb-8 border-b border-white/10 pb-6">
                        <GradientHeading>Transfer Money</GradientHeading>
                        <p className="text-slate-400 text-sm mt-2">
                            Move money between your own chequing and savings accounts.
                        </p>
                    </div>

                    {fetchError ? (
                        <div className="flex items-center gap-3 p-4 rounded-xl border border-rose-500/20 bg-rose-500/10 text-rose-300 text-sm">
                            <AlertCircle className="w-5 h-5 flex-shrink-0" />
                            <div>
                                <span className="font-semibold">Error fetching your accounts</span>
                            </div>
                        </div>
                    ) : transferableAccounts.length < 2 ? (
                        <div className="rounded-xl border border-dashed border-slate-700 bg-slate-900/30 p-8 text-center text-sm text-slate-400">
                            You need at least two active chequing or savings accounts to transfer money.
                        </div>
                    ) : (
                        <TransferForm accounts={transferableAccounts} initialFromAccountId={from} />
                    )}
                </GlassCard>
            </div>
        </main>
    );
}
