import { requireSession } from "@/lib/session";
import { Metadata } from 'next';
import { AlertCircle } from "lucide-react";
import { API_BASE_URL } from "@/lib/config";
import { Account, getBalanceOverrides, applyBalanceOverrides } from "@/lib/accounts";
import BackLink from "@/components/BackLink";
import GlassCard from "@/components/GlassCard";
import GradientHeading from "@/components/GradientHeading";
import PaymentForm from "./PaymentForm";

export const metadata: Metadata = {
  title: 'Nano-Bank - Credit Card Payment',
};

export default async function CreditCardPaymentPage() {
    const { accessToken } = await requireSession();

    let accounts: Account[] = [];
    let fetchError = false;
    try {
        const response = await fetch(`${API_BASE_URL}/api/v1/accounts`, {
            headers: { Authorization: `Bearer ${accessToken}` },
            cache: "no-store",
        });
        if (response.ok) {
            const rawAccounts = await response.json();
            const overrides = await getBalanceOverrides();
            accounts = applyBalanceOverrides(rawAccounts, overrides);
        } else {
            console.error(`Failed to fetch accounts: ${response.status}`);
            fetchError = true;
        }
    } catch (error) {
        console.error("Failed to fetch accounts:", error);
        fetchError = true;
    }

    // Filter accounts
    const fundingAccounts = accounts.filter(
        (a) => (a.account_type === "chequing" || a.account_type === "savings") && a.status === "active"
    );

    const creditCards = accounts.filter(
        (a) => a.account_type === "credit_card" && a.status === "active"
    );

    return (
        <main className="relative z-10 flex-1 flex flex-col items-center justify-center px-6 py-12">
            <div className="w-full max-w-3xl">
                <BackLink href="/dashboard/credit">Back to Credit Cards</BackLink>

                <GlassCard>
                    <div className="mb-8 border-b border-white/10 pb-6">
                        <GradientHeading>Make a Card Payment</GradientHeading>
                        <p className="text-slate-400 text-sm mt-2">
                            Pay off your credit card balance from your chequing or savings account.
                        </p>
                    </div>

                    {fetchError ? (
                        <div className="flex items-center gap-3 p-4 rounded-xl border border-rose-500/20 bg-rose-500/10 text-rose-300 text-sm">
                            <AlertCircle className="w-5 h-5 flex-shrink-0" />
                            <div>
                                <span className="font-semibold">Error fetching your accounts</span>
                            </div>
                        </div>
                    ) : fundingAccounts.length === 0 ? (
                        <div className="rounded-xl border border-dashed border-slate-700 bg-slate-900/30 p-8 text-center text-sm text-slate-400">
                            You need an active chequing or savings account with a balance to make a payment.
                        </div>
                    ) : creditCards.length === 0 ? (
                        <div className="rounded-xl border border-dashed border-slate-700 bg-slate-900/30 p-8 text-center text-sm text-slate-400">
                            You do not have any active credit cards to pay.
                        </div>
                    ) : (
                        <PaymentForm fundingAccounts={fundingAccounts} creditCards={creditCards} />
                    )}
                </GlassCard>
            </div>
        </main>
    );
}
