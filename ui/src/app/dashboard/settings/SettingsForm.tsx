"use client";

import React, { useState } from "react";
import { useRouter } from "next/navigation";
import { toast } from "sonner";
import { ShieldAlert, KeyRound, Contact } from "lucide-react";
import { updateSettingsAction } from "@/actions/profile";
import SubmitButton from "@/components/SubmitButton";
import { CustomerProfile } from "@/lib/session";

interface SettingsFormProps {
  profile: CustomerProfile;
}

export default function SettingsForm({ profile }: SettingsFormProps) {
  const router = useRouter();
  const [loading, setLoading] = useState(false);
  const [formData, setFormData] = useState({
    firstName: profile.first_name,
    lastName: profile.last_name,
    email: profile.email,
    phoneNumber: profile.phone_number || "",
    dateOfBirth: profile.date_of_birth || "",
    sin: profile.sin || "",
    currentPassword: "",
    newPassword: "",
    confirmPassword: "",
  });

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const { name, value } = e.target;
    setFormData((prev) => ({ ...prev, [name]: value }));
  };

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setLoading(true);

    const submissionData = new FormData();
    Object.entries(formData).forEach(([key, value]) => {
      submissionData.append(key, value);
    });

    try {
      const response = await updateSettingsAction(submissionData);
      if (response.success) {
        toast.success(response.message);
        router.push("/dashboard");
        return;
      }
      toast.error(response.message);
    } catch (error) {
      console.error("Update settings error:", error);
      toast.error("An unexpected error occurred while updating settings.");
    } finally {
      setLoading(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-8 w-full">
      {/* Contact Info Section */}
      <div className="space-y-5">
        <div className="flex items-center gap-2 pb-2 border-b border-white/5">
          <Contact className="w-5 h-5 text-nanobank-blue-sky" />
          <h2 className="text-lg font-bold text-white">Contact & Personal Info</h2>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          {/* First Name */}
          <div className="space-y-2">
            <label htmlFor="firstName" className="text-xs font-semibold tracking-wide text-slate-300">
              First Name
            </label>
            <input
              id="firstName"
              name="firstName"
              type="text"
              required
              value={formData.firstName}
              onChange={handleChange}
              className="w-full px-4 py-3 rounded-lg border border-slate-700 bg-slate-900/50 hover:border-slate-500 focus:border-nanobank-blue-sky focus:outline-none transition-colors duration-200 text-sm placeholder:text-slate-500"
            />
          </div>

          {/* Last Name */}
          <div className="space-y-2">
            <label htmlFor="lastName" className="text-xs font-semibold tracking-wide text-slate-300">
              Last Name
            </label>
            <input
              id="lastName"
              name="lastName"
              type="text"
              required
              value={formData.lastName}
              onChange={handleChange}
              className="w-full px-4 py-3 rounded-lg border border-slate-700 bg-slate-900/50 hover:border-slate-500 focus:border-nanobank-blue-sky focus:outline-none transition-colors duration-200 text-sm placeholder:text-slate-500"
            />
          </div>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          {/* Email (Username) */}
          <div className="space-y-2">
            <label htmlFor="email" className="text-xs font-semibold tracking-wide text-slate-300">
              Email Address (Username)
            </label>
            <input
              id="email"
              name="email"
              type="email"
              required
              value={formData.email}
              onChange={handleChange}
              className="w-full px-4 py-3 rounded-lg border border-slate-700 bg-slate-900/50 hover:border-slate-500 focus:border-nanobank-blue-sky focus:outline-none transition-colors duration-200 text-sm placeholder:text-slate-500"
            />
          </div>

          {/* Phone Number */}
          <div className="space-y-2">
            <label htmlFor="phoneNumber" className="text-xs font-semibold tracking-wide text-slate-300">
              Phone Number
            </label>
            <input
              id="phoneNumber"
              name="phoneNumber"
              type="tel"
              required
              value={formData.phoneNumber}
              onChange={handleChange}
              placeholder="e.g. 555-019-2834"
              className="w-full px-4 py-3 rounded-lg border border-slate-700 bg-slate-900/50 hover:border-slate-500 focus:border-nanobank-blue-sky focus:outline-none transition-colors duration-200 text-sm placeholder:text-slate-500"
            />
          </div>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          {/* Date of Birth */}
          <div className="space-y-2">
            <label htmlFor="dateOfBirth" className="text-xs font-semibold tracking-wide text-slate-300">
              Date of Birth
            </label>
            <input
              id="dateOfBirth"
              name="dateOfBirth"
              type="date"
              required
              value={formData.dateOfBirth}
              onChange={handleChange}
              className="w-full px-4 py-3 rounded-lg border border-slate-700 bg-slate-900/50 hover:border-slate-500 focus:border-nanobank-blue-sky focus:outline-none transition-colors duration-200 text-sm placeholder:text-slate-500"
            />
          </div>

          {/* SIN (Social Insurance Number) */}
          <div className="space-y-2">
            <label htmlFor="sin" className="text-xs font-semibold tracking-wide text-slate-300">
              SIN (9 Digits)
            </label>
            <input
              id="sin"
              name="sin"
              type="text"
              maxLength={9}
              value={formData.sin}
              onChange={handleChange}
              placeholder="e.g. 123456789"
              className="w-full px-4 py-3 rounded-lg border border-slate-700 bg-slate-900/50 hover:border-slate-500 focus:border-nanobank-blue-sky focus:outline-none transition-colors duration-200 text-sm placeholder:text-slate-500"
            />
          </div>
        </div>
      </div>

      {/* Security Info Section */}
      <div className="space-y-5">
        <div className="flex items-center gap-2 pb-2 border-b border-white/5">
          <KeyRound className="w-5 h-5 text-nanobank-orange-deep" />
          <h2 className="text-lg font-bold text-white">Change Password</h2>
        </div>

        <div className="p-4 rounded-xl border border-amber-500/10 bg-amber-500/5 flex gap-3 text-xs text-slate-400">
          <ShieldAlert className="w-5 h-5 text-nanobank-amber-deep flex-shrink-0" />
          <div>
            Leave password fields blank if you do not wish to change your password.
          </div>
        </div>

        <div className="space-y-4">
          {/* Current Password */}
          <div className="space-y-2">
            <label htmlFor="currentPassword" className="text-xs font-semibold tracking-wide text-slate-300">
              Current Password
            </label>
            <input
              id="currentPassword"
              name="currentPassword"
              type="password"
              value={formData.currentPassword}
              onChange={handleChange}
              placeholder="••••••••"
              autoComplete="current-password"
              className="w-full px-4 py-3 rounded-lg border border-slate-700 bg-slate-900/50 hover:border-slate-500 focus:border-nanobank-blue-sky focus:outline-none transition-colors duration-200 text-sm placeholder:text-slate-500"
            />
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            {/* New Password */}
            <div className="space-y-2">
              <label htmlFor="newPassword" className="text-xs font-semibold tracking-wide text-slate-300">
                New Password
              </label>
              <input
                id="newPassword"
                name="newPassword"
                type="password"
                value={formData.newPassword}
                onChange={handleChange}
                placeholder="••••••••"
                autoComplete="new-password"
                className="w-full px-4 py-3 rounded-lg border border-slate-700 bg-slate-900/50 hover:border-slate-500 focus:border-nanobank-blue-sky focus:outline-none transition-colors duration-200 text-sm placeholder:text-slate-500"
              />
            </div>

            {/* Confirm New Password */}
            <div className="space-y-2">
              <label htmlFor="confirmPassword" className="text-xs font-semibold tracking-wide text-slate-300">
                Confirm New Password
              </label>
              <input
                id="confirmPassword"
                name="confirmPassword"
                type="password"
                value={formData.confirmPassword}
                onChange={handleChange}
                placeholder="••••••••"
                autoComplete="new-password"
                className="w-full px-4 py-3 rounded-lg border border-slate-700 bg-slate-900/50 hover:border-slate-500 focus:border-nanobank-blue-sky focus:outline-none transition-colors duration-200 text-sm placeholder:text-slate-500"
              />
            </div>
          </div>
        </div>
      </div>

      <SubmitButton loading={loading} loadingText="Saving Changes...">
        Save Settings
      </SubmitButton>
    </form>
  );
}
