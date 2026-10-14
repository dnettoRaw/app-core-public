// Application-owned deterministic game state; no timers, DOM, AI or Runtime authority.
export function initial(best=0){return {phase:'ready',revision:0,score:0,best,y:0,vy:0,ticks:0,spawn:85,obstacles:[],seed:7};}
export function command(state,action,revision=state.revision){
 if(revision!==state.revision) return false;
 if(action==='restart'){Object.assign(state,initial(state.best));state.revision=revision+1;return true;}
 if(action==='pause'&&['playing','paused'].includes(state.phase)){state.phase=state.phase==='playing'?'paused':'playing';state.revision++;return true;}
 if(action==='jump'){
  if(state.phase==='ready'||state.phase==='crashed'){const best=state.best;Object.assign(state,initial(best));state.phase='playing';}
  if(state.phase!=='playing'||state.y>0)return false;
  state.vy=620;state.revision=revision+1;return true;
 }
 return false;
}
export function step(state,dt){
 if(state.phase!=='playing')return;
 if(!Number.isFinite(dt)||dt<=0||dt>1/30)throw new RangeError('bounded physics step');
 state.ticks++;state.score+=dt*10;state.best=Math.max(state.best,Math.floor(state.score));
 state.y=Math.max(0,state.y+state.vy*dt);state.vy-=1800*dt;if(state.y===0)state.vy=0;
 const speed=260+Math.min(220,state.score*2);
 for(const o of state.obstacles)o.x-=speed*dt;
 state.obstacles=state.obstacles.filter(o=>o.x+o.w>0);
 if(--state.spawn<=0){state.seed=(state.seed*1664525+1013904223)>>>0;const high=state.seed%3===0;state.obstacles.push({x:960,w:high?25:38,h:high?54:35});state.spawn=75+state.seed%50;}
 // Inset hitbox makes the visual silhouette forgiving; at most 8 obstacles retained.
 if(state.obstacles.length>8)throw new RangeError('obstacle budget');
 if(state.obstacles.some(o=>o.x<117&&o.x+o.w>88&&state.y<o.h-5))state.phase='crashed';
}
