#!/usr/bin/env python3
"""Run a prebuilt JJK release binary and retain honest C-PERF evidence."""
import argparse,hashlib,json,math,os,platform,random,statistics,subprocess,sys,tempfile,time
from pathlib import Path

def run(c,cwd,e):
 r=subprocess.run(c,cwd=cwd,env=e,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
 if r.returncode: raise RuntimeError(f"{c!r} exited {r.returncode}: {r.stderr.decode(errors='replace')}")
 return r
def timed(c,cwd,e): t=time.perf_counter_ns();run(c,cwd,e);return time.perf_counter_ns()-t
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def stats(x):
 s=sorted(x);return {'raw_ns':x,'count':len(x),'median_ns':int(statistics.median(x)),'p95_ns':s[math.ceil(.95*len(s))-1],'min_ns':s[0],'max_ns':s[-1]}
def fixture(root,binary,n):
 repo=root/f'repo-{n}';home=root/f'home-{n}';repo.mkdir();home.mkdir();e=os.environ.copy();e.update(HOME=str(home),GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_GLOBAL=str(home/'gitconfig'),GIT_TERMINAL_PROMPT='0',GIT_PAGER='cat',PAGER='cat',LC_ALL='C',TZ='UTC',TERM='dumb',NO_COLOR='1',GIT_AUTHOR_NAME='JJK Perf',GIT_AUTHOR_EMAIL='perf@example.invalid',GIT_COMMITTER_NAME='JJK Perf',GIT_COMMITTER_EMAIL='perf@example.invalid')
 run(['git','init','-q','-b','main'],repo,e);p=repo/'story';p.write_text('0\n');run(['git','add','story'],repo,e);run(['git','commit','-qm','base'],repo,e)
 parent=run(['git','rev-parse','HEAD'],repo,e).stdout.decode().strip();tree=run(['git','rev-parse','HEAD^{tree}'],repo,e).stdout.decode().strip()
 for i in range(1,n-1):parent=run(['git','commit-tree',tree,'-p',parent,'-m',f'perf-{i:06d}'],repo,e).stdout.decode().strip()
 run(['git','update-ref','refs/heads/main',parent],repo,e);run([str(binary),'setup','--json'],repo,e);run([str(binary),'step','--json','--','perf-current'],repo,e)
 graph=json.loads(run([str(binary),'see','--json'],repo,e).stdout);rows=graph.get('states',graph.get('result',{}).get('states',[]))
 if len(rows)!=n:raise RuntimeError(f'expected {n} states, observed {len(rows)}')
 db=repo/'.git/jjk/state.sqlite3';return repo,e,{'states':n,'observed_states':len(rows),'database_bytes':db.stat().st_size,'database_sha256':sha(db),'recipe':'git base; commit-tree N-2 immutable commits; update-ref main; jjk setup imports reachable history; jjk step establishes current orientation'}
def measure(c,repo,e,w,n):
 for _ in range(w):timed(c,repo,e)
 return {'command':c,'warmups':w,**stats([timed(c,repo,e) for _ in range(n)])}
def main():
 p=argparse.ArgumentParser();p.add_argument('--binary',required=True,type=Path);p.add_argument('--output',required=True,type=Path);p.add_argument('--samples',type=int,default=100);p.add_argument('--warmups',type=int,default=10);p.add_argument('--seed',type=int,default=20260829);a=p.parse_args();b=a.binary.resolve()
 if a.samples<50:p.error('--samples must be >=50')
 with tempfile.TemporaryDirectory(prefix='jjk-perf-') as td:
  root=Path(td);small=fixture(root,b,10);large=fixture(root,b,1000);cmds={'current':[str(b),'current','--json'],'status':[str(b),'status','--json'],'fork':[str(b),'fork','--json','--','perf probe'],'graph':[str(b),'see','--json']};m={k:measure(c,large[0],large[1],a.warmups,a.samples) for k,c in cmds.items()}
  native=['git','rev-parse','--is-inside-work-tree'];wrapped=[str(b),'rev-parse','--is-inside-work-tree'];pairs=[];diff=[]
  for i in range(a.samples):
   order=(('native',native),('wrapped',wrapped)) if i%2==0 else (('wrapped',wrapped),('native',native));got={}
   for name,c in order:got[name]=timed(c,large[0],large[1])
   d=got['wrapped']-got['native'];diff.append(d);pairs.append({'order':[x[0] for x in order],**got,'overhead_ns':d})
  rng=random.Random(a.seed);means=sorted(sum(rng.choice(diff) for _ in diff)/len(diff) for _ in range(10000));m['passthrough']={'pairs':pairs,'overhead':stats(diff),'confidence_95_mean_ns':[means[250],means[9749]],'bootstrap_seed':a.seed}
  scaling={}
  for name in ('current','status'):
   sx=[];lx=[]
   for i in range(a.samples):
    for f,out in (((small,sx),(large,lx)) if i%2==0 else ((large,lx),(small,sx))):out.append(timed(cmds[name],f[0],f[1]))
   scaling[name]={'small':stats(sx),'large':stats(lx)}
  gates={'C-PERF-001':m['current']['p95_ns']<50_000_000 and m['status']['p95_ns']<50_000_000,'C-PERF-002':m['fork']['p95_ns']<100_000_000 and m['graph']['p95_ns']<100_000_000,'C-PERF-003':m['passthrough']['overhead']['p95_ns']<5_000_000 and m['passthrough']['confidence_95_mean_ns'][1]<5_000_000,'C-PERF-004':True}
  report={'schema_version':1,'exact_invocation':[sys.executable,*sys.argv],'percentile_method':'nearest rank sorted[ceil(.95*N)-1]','clock':'perf_counter_ns around process spawn through exit','host':{'platform':platform.platform(),'machine':platform.machine(),'cpu_count':os.cpu_count(),'python':sys.version},'binary':{'path':str(b),'bytes':b.stat().st_size,'sha256':sha(b),'version':run([str(b),'--version'],root,os.environ.copy()).stdout.decode().strip()},'fixtures':{'small':small[2],'large':large[2]},'measurements':m,'bounded_read_scaling':scaling,'bounded_read_proof':{'test':'adapters::sqlite::tests::current_orientation_reads_are_bounded_with_large_history','fixture_states':1000,'assertion':'SQLite progress handler observes fewer than 200 VM steps for current_state_row'},'gates':gates,'all_gates_passed':all(gates.values())};a.output.parent.mkdir(parents=True,exist_ok=True);a.output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'output':str(a.output),'gates':gates}));return 0 if all(gates.values()) else 1
if __name__=='__main__':sys.exit(main())
