"use client";

import { useState, useEffect, useRef } from "react";
import Link from "next/link";
import { User, Settings, LogOut } from "lucide-react";
import { logoutAction } from "@/actions/auth";

interface CustomerProfile {
  first_name: string;
  last_name: string;
  email: string;
}

interface ProfileDropdownProps {
  profile: CustomerProfile | null;
}

export default function ProfileDropdown({ profile }: ProfileDropdownProps) {
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  // Toggle dropdown
  const toggleDropdown = () => setIsOpen((prev) => !prev);

  // Close dropdown on click outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    };

    if (isOpen) {
      document.addEventListener("mousedown", handleClickOutside);
    }
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [isOpen]);

  return (
    <div className="relative inline-block text-left" ref={dropdownRef}>
      {/* Dropdown Toggle Button */}
      <button
        onClick={toggleDropdown}
        className="flex items-center justify-center w-8 h-8 rounded-full bg-nanobank-blue-sky/20 hover:bg-nanobank-blue-sky/35 text-nanobank-blue-sky transition-all duration-200 select-none cursor-pointer focus:outline-none focus:ring-2 focus:ring-nanobank-blue-sky/40"
        aria-haspopup="true"
        aria-expanded={isOpen}
      >
        <span className="text-xs font-bold uppercase tracking-wider">
          {profile ? profile.first_name[0] + profile.last_name[0] : <User className="w-4 h-4" />}
        </span>
      </button>

      {/* Dropdown Menu */}
      {isOpen && (
        <div className="absolute right-0 mt-2 w-48 rounded-xl border border-white/10 bg-slate-950/95 backdrop-blur-md shadow-2xl py-1 z-50 ring-1 ring-black/5 animate-in fade-in slide-in-from-top-2 duration-100 origin-top-right">
          {profile && (
            <div className="px-4 py-2.5 border-b border-white/5">
              <p className="text-xs text-slate-500 font-medium">Signed in as</p>
              <p className="text-xs font-semibold text-slate-200 truncate mt-0.5">{profile.email}</p>
            </div>
          )}
          
          <Link
            href="/dashboard/settings"
            onClick={() => setIsOpen(false)}
            className="flex items-center gap-2 px-4 py-2 text-sm text-slate-300 hover:text-white hover:bg-white/5 transition-colors cursor-pointer"
          >
            <Settings className="w-4 h-4 text-slate-400" />
            <span>Settings</span>
          </Link>
          
          <form action={logoutAction} className="w-full">
            <button
              type="submit"
              className="flex w-full items-center gap-2 px-4 py-2 text-sm text-rose-400 hover:text-rose-300 hover:bg-rose-500/10 transition-colors cursor-pointer text-left font-medium"
            >
              <LogOut className="w-4 h-4" />
              <span>Log out</span>
            </button>
          </form>
        </div>
      )}
    </div>
  );
}
