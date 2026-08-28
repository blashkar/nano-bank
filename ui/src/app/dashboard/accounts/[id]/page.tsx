import { requireSession } from "@/lib/session";
import { Metadata } from 'next';
import { ArrowLeftRight, ArrowDownToLine, AlertCircle } from "lucide-react";
import { API_BASE_URL } from "@/lib/config";
import { Account } from "@/lib/accounts";
import { TransactionResponse, TransactionHistoryResponse } from "@/lib/transactions";
import BackLink from "@/components/BackLink";
import GlassCard from "@/components/GlassCard";
import GradientHeading from "@/components/GradientHeading";
import Link from "next/link";

export const metadata: Metadata = {
  title: 'Nano-Bank - Account Details',
};

type Props = {
  params: Promise<{ id: string }>;
};

const TRANSACTIONS_LIMIT = 10;

const formatCurrency = (val: number) => {
  return new Intl.NumberFormat("en-CA", {
    style: "currency",
    currency: "CAD",
  }).format(val);
};

const formatAccountNumber = (num: string) => {
  // Formats as 4-4-4: "1234 5678 9012"
  return num.replace(/(\d{4})(?=\d)/g, "$1 ");
};

const formatDate = (iso: string) => {
  return new Intl.DateTimeFormat("en-CA", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(iso));
};

const transactionTypeLabel = (type: string) => {
  return type
    .split("_")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
};

const statusBadgeClasses = (status: string) => {
  switch (status) {
    case "Completed":
      return "bg-emerald-500/10 text-emerald-400 border border-emerald-500/20";
    case "Pending":
      return "bg-nanobank-blue-sky/10 text-nanobank-blue-sky border border-nanobank-blue-sky/20";
    case "Failed":
    case "Cancelled":
      return "bg-rose-500/10 text-rose-400 border border-rose-500/20";
    default:
      return "bg-slate-500/10 text-slate-400 border border-slate-500/20";
  }
};

export default async function AccountDetailsPage({ params }: Props) {
    const { accessToken } = await requireSession();
    const { id } = await params;

    let account: Account | null = null;
    let accountError = false;
    try {
        const response = await fetch(`${API_BASE_URL}/api/v1/accounts/${id}`, {
            headers: { Authorization: `Bearer ${accessToken}` },
            cache: "no-store",
        });
        if (response.ok) {
            account = await response.json();
        } else {
            console.error(`Failed to fetch account: ${response.status}`);
            accountError = true;
        }
    } catch (error) {
        console.error("Failed to fetch account:", error);
        accountError = true;
    }

    let transactions: TransactionResponse[] = [];
    let transactionsError = false;
    try {
        const response = await fetch(
            `${API_BASE_URL}/api/v1/transactions?account_id=${id}&limit=${TRANSACTIONS_LIMIT}`,
            {
                headers: { Authorization: `Bearer ${accessToken}` },
                cache: "no-store",
            }
        );
        if (response.ok) {
            const body: TransactionHistoryResponse = await response.json();
            transactions = body.transactions;
        } else {
            console.error(`Failed to fetch transactions: ${response.status}`);
            transactionsError = true;
        }
    } catch (error) {
        console.error("Failed to fetch transactions:", error);
        transactionsError = true;
    }

    return (
        <main className="relative z-10 flex-1 flex flex-col items-center justify-center px-6 py-12">
            <div className="w-full max-w-3xl">
                <BackLink href="/dashboard/accounts">Back to Accounts</BackLink>

                {/* Details Card */}
                <GlassCard className="mb-6">
                    <div className="mb-8 border-b border-white/10 pb-6">
                        <GradientHeading>Account Details</GradientHeading>
                        <p className="text-slate-400 text-xs mt-1 font-mono">
                            Account ID: {id}
                        </p>
                    </div>

                    {accountError || !account ? (
                        <div className="flex items-center gap-3 p-4 rounded-xl border border-rose-500/20 bg-rose-500/10 text-rose-300 text-sm">
                            <AlertCircle className="w-5 h-5 flex-shrink-0" />
                            <div>
                                <span className="font-semibold">Error fetching account details</span>
                            </div>
                        </div>
                    ) : (
                        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                            <div>
                                <div className="flex items-center gap-2">
                                    <h3 className="text-lg font-bold text-white capitalize">
                                        {account.account_type.replace("_", " ")} Account
                                    </h3>
                                    <span className={`text-[10px] font-semibold px-2 py-0.5 rounded-full capitalize ${
                                        account.status === "active" ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/20" :
                                        account.status === "frozen" ? "bg-nanobank-blue-sky/10 text-nanobank-blue-sky border border-nanobank-blue-sky/20" :
                                        "bg-rose-500/10 text-rose-400 border border-rose-500/20"
                                    }`}>
                                        {account.status}
                                    </span>
                                </div>
                                <p className="text-xs text-slate-400 mt-1 font-mono">
                                    Account No. {formatAccountNumber(account.account_number)}
                                </p>
                            </div>
                            <div className="text-left sm:text-right">
                                <span className="text-slate-500 text-[10px] font-semibold uppercase tracking-wider block sm:inline">
                                    Available Balance
                                </span>
                                <p className="text-2xl font-extrabold text-white mt-0.5">
                                    {formatCurrency(parseFloat(account.balance))}
                                </p>
                                {(account.account_type === "chequing" || account.account_type === "savings") &&
                                    account.status === "active" && (
                                        <div className="flex items-center gap-2 mt-2 justify-end">
                                            <Link
                                                href={`/dashboard/accounts/deposit?account=${account.account_id}`}
                                                className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-white/10 bg-white/5 text-slate-200 hover:bg-white/10 transition-all text-xs font-semibold cursor-pointer"
                                            >
                                                <ArrowDownToLine className="w-3.5 h-3.5" />
                                                Deposit
                                            </Link>
                                            <Link
                                                href={`/dashboard/accounts/transfer?from=${account.account_id}`}
                                                className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-white/10 bg-white/5 text-slate-200 hover:bg-white/10 transition-all text-xs font-semibold cursor-pointer"
                                            >
                                                <ArrowLeftRight className="w-3.5 h-3.5" />
                                                Transfer
                                            </Link>
                                        </div>
                                    )}
                            </div>
                        </div>
                    )}
                </GlassCard>

                {/* Transactions Card */}
                <GlassCard>
                    <div className="mb-6 border-b border-white/10 pb-4 flex items-center justify-between">
                        <h2 className="text-lg font-bold text-white">Recent Transactions</h2>
                        <Link
                            href={`/dashboard/accounts/${id}/transactions`}
                            className="text-xs font-semibold text-nanobank-blue-sky hover:underline"
                        >
                            View All
                        </Link>
                    </div>

                    {transactionsError ? (
                        <div className="flex items-center gap-3 p-4 rounded-xl border border-rose-500/20 bg-rose-500/10 text-rose-300 text-sm">
                            <AlertCircle className="w-5 h-5 flex-shrink-0" />
                            <div>
                                <span className="font-semibold">Error fetching transactions</span>
                            </div>
                        </div>
                    ) : transactions.length === 0 ? (
                        <div className="rounded-xl border border-dashed border-slate-700 bg-slate-900/30 p-8 text-center text-sm text-slate-400">
                            No transactions on this account yet.
                        </div>
                    ) : (
                        <div className="overflow-x-auto -mx-2">
                            <table className="w-full text-sm border-collapse">
                                <thead>
                                    <tr className="text-left text-[10px] uppercase tracking-wider text-slate-500 border-b border-white/10">
                                        <th className="px-2 py-3 font-semibold whitespace-nowrap">Date</th>
                                        <th className="px-2 py-3 font-semibold">Description</th>
                                        <th className="px-2 py-3 font-semibold whitespace-nowrap">Type</th>
                                        <th className="px-2 py-3 font-semibold whitespace-nowrap">Status</th>
                                        <th className="px-2 py-3 font-semibold text-right whitespace-nowrap">Amount</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {transactions.map((txn) => {
                                        const myEntry = txn.entries.find((e) => e.account_id === id);
                                        const isCredit = myEntry?.entry_type === "Credit";
                                        const signedAmount = (isCredit ? 1 : -1) * parseFloat(txn.amount);

                                        return (
                                            <tr
                                                key={txn.transaction_id}
                                                className="border-b border-white/5 hover:bg-white/5 transition-colors"
                                            >
                                                <td className="px-2 py-3 text-slate-400 whitespace-nowrap align-top">
                                                    {formatDate(txn.created_at)}
                                                </td>
                                                <td className="px-2 py-3 align-top">
                                                    <Link
                                                        href={`/dashboard/transactions/${txn.transaction_id}?account=${id}`}
                                                        className="text-white font-medium hover:underline hover:text-nanobank-blue-sky transition-colors"
                                                    >
                                                        {txn.description || transactionTypeLabel(txn.transaction_type)}
                                                    </Link>
                                                </td>
                                                <td className="px-2 py-3 text-slate-400 whitespace-nowrap align-top">
                                                    {transactionTypeLabel(txn.transaction_type)}
                                                </td>
                                                <td className="px-2 py-3 align-top">
                                                    <span
                                                        className={`text-[10px] font-semibold px-2 py-0.5 rounded-full whitespace-nowrap ${statusBadgeClasses(txn.status)}`}
                                                    >
                                                        {txn.status}
                                                    </span>
                                                </td>
                                                <td
                                                    className={`px-2 py-3 text-right font-bold whitespace-nowrap align-top ${isCredit ? "text-emerald-400" : "text-white"}`}
                                                >
                                                    {isCredit ? "+" : "-"}
                                                    {formatCurrency(Math.abs(signedAmount))}
                                                </td>
                                            </tr>
                                        );
                                    })}
                                </tbody>
                            </table>
                        </div>
                    )}
                </GlassCard>
            </div>
        </main>
    );
}
