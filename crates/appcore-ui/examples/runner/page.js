import {initial,command,step} from './model.mjs';
const canvas=document.querySelector('#game'),ctx=canvas.getContext('2d');
const state=initial();let previous=0,accumulator=0,visiblePhase='';
const send=action=>{command(state,action);update();};
document.querySelector('#jump').onclick=()=>send('jump');document.querySelector('#pause').onclick=()=>send('pause');document.querySelector('#restart').onclick=()=>send('restart');
window.addEventListener('keydown',e=>{if(e.repeat)return;if(['Space','KeyP','KeyR'].includes(e.code)){e.preventDefault();send({Space:'jump',KeyP:'pause',KeyR:'restart'}[e.code]);}});
document.addEventListener('visibilitychange',()=>{if(document.hidden&&state.phase==='playing')send('pause');previous=0;accumulator=0;});
function update(){
 if(visiblePhase!==state.phase){document.querySelector('#status').textContent={ready:'Ready',playing:'Playing',paused:'Paused',crashed:'Crashed — jump to try again'}[state.phase];visiblePhase=state.phase;}
 document.querySelector('#score').textContent=String(Math.floor(state.score)).padStart(5,'0');document.querySelector('#best').textContent=String(state.best).padStart(5,'0');
 document.querySelector('#pause').disabled=!['playing','paused'].includes(state.phase);document.querySelector('#pause').textContent=state.phase==='paused'?'Resume':'Pause';
}
function paint(){
 ctx.fillStyle='#f7faf5';ctx.fillRect(0,0,960,320);ctx.fillStyle='#e1eddf';
 for(let i=0;i<5;i++){const x=((i*230-state.score*8)%1150+1150)%1150-100;ctx.fillRect(x,50+(i%2)*24,64,8);ctx.fillRect(x+12,42+(i%2)*24,34,8);}
 ctx.strokeStyle='#adc7ad';ctx.beginPath();ctx.moveTo(0,252);ctx.lineTo(960,252);ctx.stroke();
 ctx.fillStyle='#759b79';for(let i=0;i<36;i++)ctx.fillRect((i*29-state.score*23)%960,264+i%3*7,8,2);
 ctx.fillStyle='#245c46';const y=208-state.y;
 ctx.fillRect(86,y+12,30,27);ctx.fillRect(105,y,27,18);ctx.fillRect(82,y+8,9,9);ctx.fillRect(114,y+24,13,6);ctx.clearRect(123,y+4,4,4);
 const leg=state.phase==='playing'&&state.y===0?Math.floor(state.ticks/7)%2*5:0;ctx.fillRect(88,y+36,8,9-leg);ctx.fillRect(106,y+36,8,4+leg);
 ctx.fillStyle='#ac7840';for(const o of state.obstacles){ctx.fillRect(o.x,252-o.h,o.w,o.h);ctx.fillRect(o.x-5,252-o.h+13,7,15);ctx.fillRect(o.x+o.w-2,252-o.h+8,7,17);}
 if(state.phase!=='playing'){ctx.fillStyle='#173d2e';ctx.textAlign='center';ctx.font='bold 22px system-ui';ctx.fillText({ready:'A little leap goes a long way.',paused:'Take a breath.',crashed:'One more run?'}[state.phase],480,145);ctx.font='13px system-ui';ctx.fillText(state.phase==='ready'?'SPACE or Start / Jump':state.phase==='paused'?'Press P or Resume':'SPACE to restart',480,171);}
}
function frame(now){if(previous)accumulator+=Math.min(.1,(now-previous)/1000);previous=now;while(accumulator>=1/60){step(state,1/60);accumulator-=1/60;}update();paint();requestAnimationFrame(frame);}
update();requestAnimationFrame(frame);
