// Archive methods never start/terminate processes or open SQLite. FixtureJob is
// an explicit kill-on-close owner used only by the separate policy test.
using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

public static class WinBridgeTestArchiveV1 {
    static string LongPath(string path) {
        path=Path.GetFullPath(path);
        return path.StartsWith(@"\\?\",StringComparison.Ordinal) ? path : @"\\?\"+path;
    }
    [StructLayout(LayoutKind.Sequential)] struct FileInfo {
        public uint Attributes, CreationLow, CreationHigh, AccessLow, AccessHigh,
            WriteLow, WriteHigh, Volume, SizeHigh, SizeLow, Links, IndexHigh, IndexLow;
    }
    [StructLayout(LayoutKind.Sequential)] struct Accounting {
        public long User, Kernel, PeriodUser, PeriodKernel;
        public uint Faults, Total, Active, Terminated;
    }
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern SafeFileHandle CreateFileW(string path, uint access, uint share,
        IntPtr security, uint creation, uint flags, IntPtr template);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool GetFileInformationByHandle(SafeFileHandle file, out FileInfo info);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern bool MoveFileExW(string source, string target, uint flags);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern IntPtr OpenJobObjectW(uint access, bool inherit, string name);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool QueryInformationJobObject(IntPtr job, int kind,
        out Accounting info, uint size, IntPtr length);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern IntPtr CreateJobObjectW(IntPtr security, string name);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool SetInformationJobObject(IntPtr job, int kind, IntPtr info, uint size);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);

    static FileInfo Info(SafeFileHandle file) {
        FileInfo info;
        if (!GetFileInformationByHandle(file, out info)) throw new Win32Exception();
        if ((info.Attributes & 0x400) != 0) throw new IOException("Reparse entry refused");
        return info;
    }
    public static string Identity(SafeFileHandle file) {
        var i=Info(file);
        if ((i.Attributes & 0x10)==0 && i.Links!=1) throw new IOException("Hard-linked file refused");
        return i.Volume+":"+i.IndexHigh+":"+i.IndexLow;
    }
    // Ancestor pins deny rename/delete of the path used by MoveFileExW.
    public static SafeFileHandle PinDirectory(string path) {
        var h=CreateFileW(LongPath(path),0x80,3,IntPtr.Zero,3,0x02200000,IntPtr.Zero);
        if (h.IsInvalid) { int error=Marshal.GetLastWin32Error(); h.Dispose(); throw new Win32Exception(error); }
        try {
            if ((Info(h).Attributes & 0x10)==0) throw new IOException("Directory required");
            return h;
        } catch { h.Dispose(); throw; }
    }
    public static void MoveNew(string source,string target) {
        // WRITE_THROUGH only: neither REPLACE_EXISTING nor COPY_ALLOWED.
        if (!MoveFileExW(LongPath(source),LongPath(target),8)) throw new Win32Exception();
    }
    public static uint ActiveProcesses(string name) {
        var h=OpenJobObjectW(4,false,name);
        if (h==IntPtr.Zero) {
            int error=Marshal.GetLastWin32Error();
            if (error==2) return 0; // retired kernel object, not a recreated owner
            throw new Win32Exception(error);
        }
        try {
            Accounting info;
            if (!QueryInformationJobObject(h,1,out info,(uint)Marshal.SizeOf<Accounting>(),IntPtr.Zero))
                throw new Win32Exception();
            return info.Active;
        } finally { CloseHandle(h); }
    }
    // Only the policy test uses this owner; archive itself never kills a process.
    public sealed class FixtureJob : IDisposable {
        IntPtr handle;
        public FixtureJob(string name) {
            if (IntPtr.Size!=8) throw new IOException("64-bit native test required");
            handle=CreateJobObjectW(IntPtr.Zero,name);
            if (handle==IntPtr.Zero) throw new Win32Exception();
            if (Marshal.GetLastWin32Error()==183) { Dispose(); throw new IOException("Existing test Job refused"); }
            var info=Marshal.AllocHGlobal(144);
            try {
                Marshal.Copy(new byte[144],0,info,144);
                Marshal.WriteInt32(info,16,0x2000); // KILL_ON_JOB_CLOSE, test owner only
                if (!SetInformationJobObject(handle,9,info,144)) throw new Win32Exception();
            } catch { Dispose(); throw; }
            finally { Marshal.FreeHGlobal(info); }
        }
        public void Assign(IntPtr process) {
            if (!AssignProcessToJobObject(handle,process)) throw new Win32Exception();
        }
        public void Dispose() {
            if (handle!=IntPtr.Zero) { CloseHandle(handle); handle=IntPtr.Zero; }
        }
    }
}
